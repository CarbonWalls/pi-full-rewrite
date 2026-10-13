//! Full-stack subprocess E2E: the REAL `pi` binary serves HTTP, talks to the
//! REAL openai-compat provider path (fake server), executes REAL tools, and
//! streams NDJSON back over a raw TCP socket. One test, every layer.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

fn start_fake_openai(recorded: Arc<Mutex<Vec<Value>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..2 {
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
            recorded.lock().unwrap().push(serde_json::from_slice(&body).unwrap_or(Value::Null));
            let sse = |j: Value| format!("data: {j}\n\n");
            let mut out = String::new();
            out.push_str(&sse(serde_json::json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"http.txt\",\"content\":\"over http\"}"}}]},"finish_reason":null}]})));
            out.push_str(&sse(serde_json::json!({"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":4,"total_tokens":9}})));
            out.push_str("data: [DONE]\n\n");
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(out.as_bytes());
            let _ = sock.flush();
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
    panic!("serve did not come up on {port}");
}

#[test]
fn binary_serve_http_pair_with_real_provider() {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let fport = start_fake_openai(recorded.clone());
    let sport = free_port();

    let dir = std::env::temp_dir().join(format!("pi-bin-serve-{}", std::process::id()));
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
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pi");
    let mut err = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
    });

    wait_for_server(sport, &mut child);

    let mut stream = TcpStream::connect(("127.0.0.1", sport)).unwrap();
    let req = "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 46\r\n\r\n{\"id\":1,\"type\":\"prompt\",\"input\":\"create a file\"}";
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let text = String::from_utf8_lossy(&resp).into_owned();
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    assert!(body.contains("write_file"), "no tool trace in rpc stream: {body}");
    assert!(body.contains("\"type\":\"done\""), "no done frame: {body}");

    let content = std::fs::read_to_string(dir.join("http.txt")).unwrap();
    assert_eq!(content, "over http", "tool wrote via HTTP rpc");

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2, "two provider calls");
    assert!(rec[1].to_string().contains("\"role\":\"tool\""), "second call carries tool result");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
