//! `--serve-token`: every /rpc route requires the bearer token (healthz stays
//! open), enforced by the real binary.

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
            panic!("pi serve exited early: {st:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    panic!("serve not up");
}

fn get(port: u16, path: &str, token: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let auth = match token {
        Some(t) => format!("Authorization: Bearer {t}\r\n"),
        None => String::new(),
    };
    let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let text = String::from_utf8_lossy(&resp).into_owned();
    let status: u16 = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, text)
}

#[test]
fn serve_token_blocks_routes_keeps_healthz_open() {
    let dir = std::env::temp_dir().join(format!("pi-tok-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sport = free_port();

    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--serve",
            "--port",
            &sport.to_string(),
            "--serve-token",
            "s3cret",
            "--root",
            dir.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serve");
    wait_for_server(sport, &mut child);

    // healthz stays open without a token
    let (st0, _) = get(sport, "/healthz", None);
    assert_eq!(st0, 200, "healthz open");

    // /info and /sessions require the token
    let (st1, b1) = get(sport, "/info", None);
    assert_eq!(st1, 401, "info without token: {b1}");
    let (st2, _) = get(sport, "/info", Some("wrong"));
    assert_eq!(st2, 401, "wrong token");
    let (st3, b3) = get(sport, "/info", Some("s3cret"));
    assert_eq!(st3, 200, "right token: {b3}");
    assert!(b3.contains("pi"), "{b3}");

    let (st4, _) = get(sport, "/sessions", None);
    assert_eq!(st4, 401, "sessions without token");

    // /metrics: protected, but reports the counters when authorized
    let (st5, _) = get(sport, "/metrics", None);
    assert_eq!(st5, 401, "metrics without token");
    let (st6, b6) = get(sport, "/metrics", Some("s3cret"));
    assert_eq!(st6, 200, "metrics with token");
    assert!(b6.contains("pi_requests"), "{b6}");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
