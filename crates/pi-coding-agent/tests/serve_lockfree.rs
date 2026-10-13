//! Regression: /cost and /sessions must NOT wait on the agent mutex while a
//! prompt is running (the dashboard would freeze mid-turn otherwise).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Instant;

fn http_get(port: u16, path: &str, timeout_ms: u64) -> Result<String, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(std::time::Duration::from_millis(timeout_ms)))
        .map_err(|e| e.to_string())?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let r = BufReader::new(s);
    let mut body = Vec::new();
    let mut in_body = false;
    let _ = &body;
    for line in r.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if !in_body {
            if line == "\r\n" || line.is_empty() {
                in_body = true;
            }
            continue;
        }
        body.extend_from_slice(line.as_bytes());
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

#[test]
fn panels_answer_during_a_running_turn() {
    let dir = std::env::temp_dir().join(format!("pi-lf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("scn.jsonl");
    std::fs::write(&scn, "{\"contains\":\"slow\",\"text\":\"slowz\",\"delay_ms\":2500}\n").unwrap();

    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);

    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--serve",
            "--port",
            &port.to_string(),
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("serve spawn");

    // wait for the listener
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // start a slow prompt
    let slow = std::thread::spawn(move || {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = "{\"id\":1,\"type\":\"prompt\",\"input\":\"slow please\"}";
        let req = format!(
            "POST /rpc HTTP/1.1\r\nHost: x\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        s.write_all(req.as_bytes()).unwrap();
        let r = BufReader::new(s);
        let mut n = 0;
        for line in r.lines() {
            if line.unwrap().contains("done") {
                n += 1;
            }
        }
        n
    });

    // give the turn time to grab the agent lock
    std::thread::sleep(std::time::Duration::from_millis(400));

    let t0 = Instant::now();
    let cost = http_get(port, "/cost", 1500);
    let t1 = Instant::now();
    let sessions = http_get(port, "/sessions", 1500);
    let t2 = Instant::now();

    assert!(
        t1.duration_since(t0).as_millis() < 1200,
        "/cost must answer while the turn holds the lock ({:?})",
        t1.duration_since(t0)
    );
    assert!(
        t2.duration_since(t1).as_millis() < 1200,
        "/sessions must answer while the turn holds the lock ({:?})",
        t2.duration_since(t1)
    );
    let cost_txt = cost.unwrap_or_default();
    let sessions_txt = sessions.unwrap_or_default();
    assert!(cost_txt.contains("model_calls"), "cost shape: {cost_txt:?}");
    assert!(sessions_txt.contains("sessions"), "sessions shape: {sessions_txt:?}");

    let _ = slow.join(); // let the turn finish
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
