//! Concurrent `/rpc` prompts over HTTP must serialize (single agent worker):
//! both replies correct, both turns recorded, and the total wall time proves
//! no interleaving. Slow-provider behavior is simulated with `delay_ms`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Instant;

fn post(port: u16, body: &str) -> Vec<serde_json::Value> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!(
        "POST /rpc HTTP/1.1\r\nHost: x\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut r = BufReader::new(s);
    let mut events = Vec::new();
    // skip headers
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
        }
    }
    for line in r.lines() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        events.push(serde_json::from_str(&line).unwrap());
    }
    events
}

#[test]
fn concurrent_prompts_serialize_over_serve() {
    let dir = std::env::temp_dir().join(format!("pi-conc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        "{\"contains\":\"first\",\"text\":\"first-done\",\"delay_ms\":400}\n\
         {\"contains\":\"second\",\"text\":\"second-done\",\"delay_ms\":400}\n",
    )
    .unwrap();

    // find a free port
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

    // wait for /healthz
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let t0 = Instant::now();
    let h1 = std::thread::spawn(move || post(port, "{\"id\":1,\"type\":\"prompt\",\"input\":\"first\"}"));
    let h2 = std::thread::spawn(move || post(port, "{\"id\":2,\"type\":\"prompt\",\"input\":\"second\"}"));
    let e1 = h1.join().unwrap();
    let e2 = h2.join().unwrap();
    let wall = t0.elapsed().as_millis();

    let collect_text = |es: &[serde_json::Value]| -> String {
        let mut t = String::new();
        for ev in es {
            if let Some(e) = ev.get("event") {
                if let Some(n) = e.get("type").and_then(|x| x.as_str()) {
                    if n == "text" {
                        if let Some(d) = e.get("delta").and_then(|x| x.as_str()) {
                            t.push_str(d);
                        }
                    }
                }
            }
        }
        t
    };

    let t1 = collect_text(&e1);
    let t2 = collect_text(&e2);
    assert!(
        t1.contains("first-done") && t2.contains("second-done") || t1.contains("second-done") && t2.contains("first-done"),
        "both replies present, got {t1:?} and {t2:?}"
    );
    assert!(
        e1.iter().any(|v| v.get("type").and_then(|x| x.as_str()) == Some("done"))
            && e2.iter().any(|v| v.get("type").and_then(|x| x.as_str()) == Some("done")),
        "both prompts completed"
    );
    assert!(
        wall >= 800 - 50,
        "serialized (400ms + 400ms), got {wall}ms — concurrency corrupted the worker?"
    );

    // both turns are recorded in the session (no interleaved corruption)
    let sess = std::fs::read_dir(dir.join(".pi")).unwrap().next().unwrap().unwrap().path();
    let s = std::fs::read_to_string(&sess).unwrap();
    assert!(s.contains("first") && s.contains("second"), "session has both turns");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
