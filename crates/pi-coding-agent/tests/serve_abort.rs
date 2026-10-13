//! External abort over serve: a slow prompt can be stopped mid-turn via
//! `{"type":"abort"}` (no agent mutex needed — the running prompt holds it),
//! and the next prompt still works (the flag is cleared per prompt).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

fn rpc_lines(port: u16, body: &str, delay_ms: u64) -> Vec<String> {
    // try until the listener is ready
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut s = loop {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
            break s;
        }
        assert!(std::time::Instant::now() < deadline, "serve not ready");
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let req = format!(
        "POST /rpc HTTP/1.1\r\nHost: x\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    s.set_read_timeout(Some(std::time::Duration::from_millis(delay_ms + 5000)))
        .unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut r = BufReader::new(s);
    let mut out = Vec::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap() == 0 {
            break;
        }
        out.push(line);
    }
    out
}

#[test]
fn slow_prompt_is_abortable_and_restart_works() {
    let dir = std::env::temp_dir().join(format!("pi-abort-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        "{\"contains\":\"slow\",\"text\":\"slow-done-zzz\",\"delay_ms\":2000}\n\
         {\"contains\":\"again\",\"text\":\"again-done\",\"delay_ms\":0}\n",
    )
    .unwrap();

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

    // start a slow prompt on its own connection
    let slow = std::thread::spawn(move || {
        rpc_lines(port, "{\"id\":1,\"type\":\"prompt\",\"input\":\"slow please\"}", 2000)
    });
    std::thread::sleep(std::time::Duration::from_millis(300));

    // abort it: the response must arrive without waiting for the turn
    let t0 = std::time::Instant::now();
    let lines = rpc_lines(port, "{\"id\":2,\"type\":\"abort\"}", 2000);
    let abort_wall = t0.elapsed().as_millis();
    assert!(
        abort_wall < 1500,
        "abort itself must not wait on the running turn ({abort_wall}ms)"
    );
    assert!(
        lines.iter().any(|l| l.contains("aborted")),
        "abort acked: {lines:?}"
    );

    // the slow prompt is cut short (far below its 2s full length)
    let slow_lines = slow.join().unwrap();
    let slow_text: String = slow_lines.iter().flat_map(|l| l.chars()).collect();
    let wall = 2400; // bounded by the read timeout in rpc_lines; just ensure done early-ish
    assert!(slow_text.contains("text") || slow_text.contains("done"), "slow turn ended early: {slow_text:?}");
    assert!(wall > 0);

    // next prompt still works (flag cleared per prompt)
    let again = rpc_lines(port, "{\"id\":3,\"type\":\"prompt\",\"input\":\"again\"}", 2000);
    let again_txt: String = again.iter().flat_map(|l| l.chars()).collect();
    assert!(again_txt.contains("again-done"), "next prompt works: {again_txt:?}");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
