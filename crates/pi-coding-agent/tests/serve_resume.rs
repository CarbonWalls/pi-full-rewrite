//! Server-side session persistence: `pi --serve` persists turns; restarting
//! the server with `--continue` resumes the SAME session file and appends.

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

fn prompt(port: u16, id: u64, text: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = format!("{{\"id\":{id},\"type\":\"prompt\",\"input\":\"{text}\"}}");
    let req = format!(
        "POST /rpc HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    String::from_utf8_lossy(&resp).into_owned()
}

fn spawn_serve(bin: &str, port: u16, sd: &str, extra: &[&str]) -> Child {
    let mut args: Vec<String> = vec![
        "--serve".into(),
        "--port".into(),
        port.to_string(),
        "--model".into(),
        "mock/echo".into(),
        "--session-dir".into(),
        sd.into(),
    ];
    for a in extra {
        args.push(a.to_string());
    }
    Command::new(bin)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serve")
}

#[test]
fn serve_persists_sessions_across_restart() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-sver-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sd = dir.join(".pi/sessions");
    std::fs::create_dir_all(&sd).unwrap();
    let sd = sd.to_str().unwrap().to_string();

    // run 1: a fresh session
    let p1 = free_port();
    let mut s1 = spawn_serve(bin, p1, &sd, &[]);
    wait_for_server(p1, &mut s1);
    let out1 = prompt(p1, 1, "first server turn");
    assert!(out1.contains("\"type\":\"done\""), "{out1}");
    let _ = s1.kill();
    let _ = s1.wait();

    let sessions: Vec<_> = std::fs::read_dir(&sd).unwrap().flatten().collect();
    assert_eq!(sessions.len(), 1, "one session after run 1");

    // run 2: --continue resumes the same session file
    let p2 = free_port();
    let mut s2 = spawn_serve(bin, p2, &sd, &["--continue"]);
    wait_for_server(p2, &mut s2);
    let out2 = prompt(p2, 2, "second server turn");
    assert!(out2.contains("\"type\":\"done\""), "{out2}");
    let _ = s2.kill();
    let _ = s2.wait();

    let f = std::fs::read_dir(&sd).unwrap().next().unwrap().unwrap().path();
    let entries = std::fs::read_to_string(&f)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(entries, 6, "same session now holds both turns (+2 usage entries): {entries}");
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.contains("first server turn") && text.contains("second server turn"));
    let _ = std::fs::remove_dir_all(&dir);
}
