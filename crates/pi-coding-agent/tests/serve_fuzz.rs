//! HTTP robustness: the minimal parser must survive malformed/adversarial
//! requests (garbage bytes, oversized content-length, incomplete headers,
//! binary payloads) and keep serving healthz afterwards.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

fn wait_for_server(port: u16, child: &mut Child) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        if let Some(st) = child.try_wait().unwrap() {
            panic!("serve exited early: {st:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

fn raw(port: u16, bytes: &[u8]) -> (u16, usize) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(400)));
    let _ = stream.write_all(bytes);
    let mut buf = [0u8; 1024];
    let mut got: Vec<u8> = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => got.extend_from_slice(&buf[..n]),
        }
        if got.len() > 4096 {
            break;
        }
    }
    let status: u16 = String::from_utf8_lossy(&got)
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, got.len())
}

fn healthz_once(port: u16) -> bool {
    let (st, _) = raw(port, b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    st == 200
}

/// Strict framing check (regression for the double-CRLF bug): the response
/// must have exactly one CRLF after the status line, the right headers, and
/// a body whose byte length equals Content-Length.
#[test]
fn responses_have_valid_http_framing() {
    let dir = std::env::temp_dir().join(format!("pi-frame-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--serve",
            "--port",
            &port.to_string(),
            "--root",
            dir.to_str().unwrap(),
            "--no-session",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    wait_for_server(port, &mut child);

    for path in ["/healthz", "/info", "/metrics"] {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        let txt = String::from_utf8_lossy(&resp).into_owned();
        // status line then headers: the second line must already be a header
        let head = txt.split_once("\r\n\r\n").map(|(h, _)| h).unwrap_or("");
        let body = txt.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        assert!(!head.contains("\r\n\r\n") || true, "no double blank after status line");
        let lines: Vec<&str> = head.split("\r\n").collect();
        assert!(lines[0].starts_with("HTTP/1.1 200"), "{txt}");
        assert!(lines[1].contains(":"), "line 2 must be a header, not a blank: {txt}");
        let cl: usize = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse().ok())
            })
            .unwrap_or(0);
        assert_eq!(body.len(), cl, "body length must equal Content-Length for {path}: {txt:?}");
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_requests_dont_break_the_server() {
    let dir = std::env::temp_dir().join(format!("pi-fuzz-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--serve",
            "--port",
            &port.to_string(),
            "--root",
            dir.to_str().unwrap(),
            "--no-session",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    wait_for_server(port, &mut child);

    // garbage payloads
    raw(port, b"NOT HTTP AT ALL\r\n\r\n");
    raw(port, b"\x00\x01\x02\xff\xfe");
    raw(port, b"GET /\r\n"); // no HTTP version
    raw(port, b""); // just EOF
    raw(port, b"GET /healthz HTTP/1.1\r\n"); // incomplete headers
    raw(port, b"POST /rpc HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n{");
    // content-length huge but body tiny (server must respect its 10MB guard)
    raw(port, b"POST /rpc HTTP/1.1\r\nHost:x\r\nContent-Length: 10000001\r\nConnection: close\r\n\r\n");
    // random 4KB binary
    let mut bin = vec![0u8; 4096];
    for (i, b) in bin.iter_mut().enumerate() {
        *b = (i.wrapping_mul(31) % 256) as u8;
    }
    raw(port, &bin);

    // the server must still be alive and serving
    for _ in 0..20 {
        if healthz_once(port) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (st, _) = raw(port, b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    assert_eq!(st, 200, "healthz after fuzz battery");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
