//! Concurrent HTTP clients against the real `pi serve` binary: N parallel
//! POST /rpc requests must all complete (turns serialize behind the agent
//! mutex) without hangs or corruption.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn start_fake(replies: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        while replies.load(Ordering::Relaxed) > 0 {
            let (mut sock, _) = match listener.accept() {
                Ok(t) => t,
                Err(_) => continue,
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
            // text reply (no tools -> one turn, one call per client)
            let chunk = serde_json::json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"concurrent ok"},"finish_reason":null}]});
            let out = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(out.as_bytes());
            let _ = sock.flush();
            replies.fetch_sub(1, Ordering::Relaxed);
        }
    });
    port
}

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
    panic!("serve did not come up");
}

fn one_rpc(port: u16, id: u64, text: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = format!("{{\"id\":{id},\"type\":\"prompt\",\"input\":\"{text}\"}}");
    let req = format!(
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    String::from_utf8_lossy(&resp).into_owned()
}

#[test]
fn concurrent_rpc_clients_all_complete() {
    let clients = 5usize;
    let replies = Arc::new(AtomicUsize::new(clients));
    let fport = start_fake(replies.clone());
    let sport = free_port();

    let dir = std::env::temp_dir().join(format!("pi-conc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let bin = env!("CARGO_BIN_EXE_pi");
    let mut child = Command::new(bin)
        .args([
            "--serve",
            "--port",
            &sport.to_string(),
            "-m",
            &format!("custom:http://127.0.0.1:{fport}/v1:fake"),
            "--no-session",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pi serve");
    wait_for_server(sport, &mut child);

    // fire N clients concurrently
    let mut handles = Vec::new();
    for i in 0..clients {
        handles.push(std::thread::spawn(move || one_rpc(sport, i as u64 + 1, &format!("task {i}"))));
    }
    let mut done_frames = 0;
    let mut oks = 0;
    for h in handles {
        let body = h.join().unwrap();
        let body = body.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&body);
        if body.contains("\"type\":\"done\"") {
            done_frames += 1;
        }
        if body.contains("concurrent ok") {
            oks += 1;
        }
    }
    assert_eq!(done_frames, clients, "every client got a done frame");
    assert_eq!(oks, clients, "every client saw the model text");

    assert_eq!(replies.load(Ordering::Relaxed), 0, "provider served all clients");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
