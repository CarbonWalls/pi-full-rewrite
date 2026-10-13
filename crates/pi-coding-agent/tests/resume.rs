//! Session persistence round-trip at the binary level: a first run persists
//! a session file; `--resume <id>` and `--continue` re-attach to it and
//! append more entries to the SAME file.

use std::process::{Command, Stdio};

fn entry_count(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

fn newest_session(dir: &std::path::Path) -> String {
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(id) = name.strip_suffix(".jsonl") else {
            continue;
        };
        let ts = e.metadata().ok().and_then(|m| m.modified().ok());
        let ts = ts.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| ts > *t).unwrap_or(true) {
            best = Some((ts, id.to_string()));
        }
    }
    best.unwrap().1
}

fn run_pi(bin: &str, root: &std::path::Path, sd: &std::path::Path, extra: &[&str], prompt: &str) {
    let mut cmd = Command::new(bin);
    let mut args: Vec<&str> = vec!["--quiet", "--root", root.to_str().unwrap(), "--session-dir", sd.to_str().unwrap()];
    for a in extra {
        args.push(a);
    }
    args.push("--mode");
    args.push("ask");
    args.push(prompt);
    let out = cmd.args(&args).stdin(Stdio::null()).output().expect("run pi");
    assert!(
        out.status.success(),
        "pi failed: {} {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn session_persists_and_resumes() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("root");
    let sd = dir.join(".pi/sessions");
    std::fs::create_dir_all(&sd).unwrap();

    // first run creates the session
    run_pi(bin, &root, &sd, &[], "first turn");
    let id = newest_session(&sd);
    let file = sd.join(format!("{id}.jsonl"));
    assert_eq!(entry_count(&file), 3, "first run: user + assistant + usage");

    // --resume <id> appends to the SAME file
    run_pi(bin, &root, &sd, &["--resume", &id], "second turn");
    assert_eq!(entry_count(&file), 6, "resume appends in place");

    // --continue re-attaches to the newest session and appends again
    run_pi(bin, &root, &sd, &["--continue"], "third turn");
    assert_eq!(entry_count(&file), 9, "continue appends in place");

    // the session is one JSONL log: every line is a valid entry object
    let mut users = 0;
    for line in std::fs::read_to_string(&file).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSONL");
        if v["data"]["msg"]["role"] == "User" {
            users += 1;
        }
    }
    assert_eq!(users, 3, "three user turns persisted");

    let _ = std::fs::remove_dir_all(&dir);
}
