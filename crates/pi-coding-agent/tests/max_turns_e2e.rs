//! `--max-turns N`: a scenario that would keep looping tools must stop
//! after N cycles and report the cap.

use std::process::{Command, Stdio};

#[test]
fn max_turns_bounds_runaway_tool_loops() {
    let dir = std::env::temp_dir().join(format!("pi-mt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("scn.jsonl");
    // three chained tool calls: bash -> bash -> bash would loop forever
    std::fs::write(
        &scn,
        "{\"contains\":\"loop\",\"tool\":\"bash\",\"args\":{\"command\":\"echo a\"}}\n\
         {\"after_tool_contains\":\"a\",\"tool\":\"bash\",\"args\":{\"command\":\"echo b\"}}\n\
         {\"after_tool_contains\":\"b\",\"tool\":\"bash\",\"args\":{\"command\":\"echo c\"}}\n\
         {\"after_tool_contains\":\"c\",\"text\":\"done three\"}\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "-m",
            "mock/echo",
            "--max-turns",
            "2",
            "loop",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // two tool cycles run, then the cap fires
    assert!(!t.contains("done three"), "must NOT complete all three: {t}");
    assert!(
        t.to_lowercase().contains("max turns"),
        "reports the cap: {t}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
