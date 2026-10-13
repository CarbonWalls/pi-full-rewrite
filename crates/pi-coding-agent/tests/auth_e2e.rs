//! End-to-end key plumbing: `pi --login` stores a key; a turn against a fake
//! OpenAI-compatible server must carry that key as `Authorization: Bearer`.
//! (The fake server returns 401 unless the header matches.)

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn stored_key_reaches_the_wire() {
    let got_auth = Arc::new(AtomicUsize::new(0));
    let got = got_auth.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
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
        let auth_line = head
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
            .unwrap_or("");
        if auth_line.eq_ignore_ascii_case("authorization: bearer sekrit-123") {
            got.fetch_add(1, Ordering::Relaxed);
            let chunk = "{\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"authenticated\"},\"finish_reason\":null}]}";
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(body.as_bytes());
        } else {
            let _ = sock.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n..");
        }
        let _ = sock.flush();
        let _ = sock.shutdown(std::net::Shutdown::Write);
        std::thread::sleep(std::time::Duration::from_millis(80));
    });

    let dir = std::env::temp_dir().join(format!("pi-aee-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // store the key the way a user would
    let login = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--login",
            "custom=sekrit-123",
            "--root",
            dir.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(login.status.success(), "{:?}", login);

    // run a turn against the fake server with no explicit --api-key
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "-m",
            &format!("custom:http://127.0.0.1:{port}/v1:fake"),
            "hi",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("authenticated"));

    assert_eq!(got_auth.load(Ordering::Relaxed), 1, "the request carried the stored Bearer key");
    let _ = std::fs::remove_dir_all(&dir);
}
