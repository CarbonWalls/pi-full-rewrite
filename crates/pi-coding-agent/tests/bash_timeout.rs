//! Per-call bash timeout override: a scenario drives `bash` with
//! `timeout_ms: 300` on a `sleep 5` command; the tool must be killed at
//! ~300ms (not 5s) and report the timeout.

use std::process::{Command, Stdio};

#[test]
fn bash_per_call_timeout_override() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-to-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sd = dir.join(".pi/sessions");
    std::fs::create_dir_all(&sd).unwrap();
    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        concat!(
            "{\"contains\":\"run slow\",\"tool\":\"bash\",\"args\":{\"command\":\"sleep 5; echo never\",\"timeout_ms\":300}}\n",
            "{\"after_tool_contains\":\"timeout\",\"text\":\"timeout works\"}\n",
        ),
    )
    .unwrap();

    let start = std::time::Instant::now();
    let out = Command::new(bin)
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            sd.to_str().unwrap(),
            "run slow",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    let elapsed = start.elapsed();
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    assert!(
        elapsed < std::time::Duration::from_secs(4),
        "must not wait the full 5s: {:?}",
        elapsed
    );

    // the session shows the timed-out tool result
    let sess = std::fs::read_dir(&sd).unwrap().next().unwrap().unwrap().path();
    let text = std::fs::read_to_string(&sess).unwrap();
    assert!(text.contains("timeout"), "tool reported timeout: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
