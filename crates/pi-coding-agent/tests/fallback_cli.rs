//! `--fallback` at the binary level: a primary that always 503s falls back
//! to the CLI-specified backup model; the final text comes from the backup.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

fn start_fake(always_500: bool, ok_text: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || loop {
        let (mut sock, _) = match listener.accept() {
            Ok(t) => t,
            Err(_) => return,
        };
        let mut reader = BufReader::new(sock.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let cl: usize = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse().ok())
            })
            .unwrap_or(0);
        let mut body = vec![0u8; cl];
        let _ = reader.read_exact(&mut body);
        if always_500 {
            let _ = sock.write_all(b"HTTP/1.1 503 Unavailable\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n..");
        } else {
            let chunk = serde_json::json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":ok_text},"finish_reason":null}]});
            let out = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(out.as_bytes());
        }
        let _ = sock.flush();
    });
    port
}

#[test]
fn cli_fallback_switches_models() {
    let pport = start_fake(true, "");
    let bport = start_fake(false, "rescued by fallback");

    let dir = std::env::temp_dir().join(format!("pi-fbcli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "-m",
            &format!("custom:http://127.0.0.1:{pport}/v1:primary"),
            "--fallback",
            &format!("custom:http://127.0.0.1:{bport}/v1:backup"),
            "hello",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("rescued by fallback"), "saw the backup reply: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
