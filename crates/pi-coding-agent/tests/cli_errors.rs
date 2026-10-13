//! CLI error paths: unknown models, missing files, and malformed RPC input
//! must fail with clear messages (not panics).

use std::process::{Command, Stdio};

fn run(args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_pi");
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        format!("{} {}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout)),
    )
}

#[test]
fn unknown_model_errors_cleanly() {
    let dir = std::env::temp_dir().join(format!("pi-err-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (_o, all) = run(&[
        "-m",
        "nope/does-not-exist",
        "hi",
        "--root",
        dir.to_str().unwrap(),
    ]);
    assert!(all.contains("unknown model"), "clear model error: {all}");
    assert!(!all.contains("panicked"), "no panic: {all}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_errors_cleanly() {
    let (_o, all) = run(&["--file", "/definitely/not/here.md"]);
    assert!(all.contains("No such file"), "clear file error: {all}");
    assert!(!all.contains("panicked"), "no panic: {all}");
}

#[test]
fn bad_rpc_json_gets_error_frame() {
    let dir = std::env::temp_dir().join(format!("pi-errrpc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--rpc", "--root", dir.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    {
        use std::io::Write;
        let mut si = child.stdin.take().unwrap();
        si.write_all(b"not json at all\n{\"type\":\"quit\"}\n").unwrap();
        si.flush().unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.contains("bad json"), "error frame for malformed line: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
