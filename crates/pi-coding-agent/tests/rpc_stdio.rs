//! Stdio JSON-lines RPC protocol (`pi --rpc`): tools listing, a prompt turn
//! (events suppressed), model switching, and quit.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn rpc_stdio_protocol() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-rpc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sd = dir.join(".pi/sessions");
    std::fs::create_dir_all(&sd).unwrap();

    let mut child = Command::new(bin)
        .args([
            "--rpc",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            sd.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pi --rpc");

    let mut stdin = child.stdin.take().unwrap();
    let input = concat!(
        "{\"type\":\"tools\"}\n",
        "{\"id\":1,\"type\":\"prompt\",\"input\":\"rpc hi\"}\n",
        "{\"id\":2,\"type\":\"model\",\"ref\":\"mock/echo\"}\n",
        "{\"type\":\"quit\"}\n",
    );
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);

    let out = child.wait_with_output().expect("wait rpc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut types: Vec<String> = Vec::new();
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or(serde_json::Value::Null);
        if let Some(t) = v["type"].as_str() {
            types.push(t.to_string());
        }
    }
    let joined = types.join(",");
    assert!(joined.contains("ready"), "starts with ready: {joined}");
    assert!(joined.contains("tools_result"), "tools listed: {joined}");
    assert!(joined.contains("event"), "prompt streams events: {joined}");
    assert!(joined.contains("done"), "prompt ends with done: {joined}");
    assert!(joined.contains("model_result"), "model switch: {joined}");
    assert!(joined.ends_with("bye"), "finishes with bye: {joined}");

    // tools_result carries the tool names
    let tools_line = text
        .lines()
        .find(|l| l.contains("tools_result"))
        .expect("tools_result line");
    assert!(tools_line.contains("read_file") && tools_line.contains("bash"), "{tools_line}");

    let _ = std::fs::remove_dir_all(&dir);
}
