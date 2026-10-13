//! Agentic diff application: a scenario drives the agent to apply a unified
//! diff to a real file and finish with a confirmation — proving the whole
//! tool chain (scenario rule -> apply_diff -> file change) end to end.

use std::process::{Command, Stdio};

#[test]
fn agent_applies_diff_via_scenario() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-agentdiff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();

    let scn = dir.join("scn.jsonl");
    let diff = "@@ -1 +1 @@\n-hello world\n+hello there friend\n";
    let rule1 = format!(
        "{{\"contains\":\"fix the file\",\"tool\":\"apply_diff\",\"args\":{{\"file\":\"f.txt\",\"diff\":\"{}\"}}}}",
        diff.replace('"', "\\\"").replace('\n', "\\n")
    );
    let rule2 = "{\"after_tool_contains\":\"applied\",\"text\":\"diff applied cleanly\"}\n";
    std::fs::write(&scn, format!("{rule1}\n{rule2}")).unwrap();

    let out = Command::new(bin)
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "fix the file",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("diff applied cleanly"), "final confirmation: {text}");

    // the real file on disk changed
    let now = std::fs::read_to_string(dir.join("f.txt")).unwrap();
    assert!(now.contains("hello there friend"), "file patched: {now:?}");
    assert!(!now.contains("hello world\n"), "old line gone");
    let _ = std::fs::remove_dir_all(&dir);
}
