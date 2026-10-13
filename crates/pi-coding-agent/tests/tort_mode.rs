//! Torture: a tool fails (reading a missing file) in the middle of a plan,
//! and the agent must absorb the error and still complete the task.

use std::process::{Command, Stdio};

#[test]
fn agent_recovers_from_tool_error_mid_task() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-tort-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), "old content").unwrap();

    let scn = dir.join("scn.jsonl");
    // 1) plan via todo
    // 2) read a MISSING file -> ERROR tool result (must not abort the run)
    // 3) apply a real diff to a.md
    // 4) bash verify
    // 5) final text
    let diff = "--- a/a.md\n+++ b/a.md\n@@ -1 +1 @@\n-old content\n+new content\n";
    std::fs::write(
        &scn,
        format!(
            concat!(
                "{{\"contains\":\"help me\",\"tool\":\"todo\",\"args\":{{\"todos\":[{{\"content\":\"a\",\"status\":\"in_progress\"}}]}}}}\n",
                "{{\"after_tool_contains\":\"TODO\",\"tool\":\"read_file\",\"args\":{{\"path\":\"missing.txt\"}}}}\n",
                "{{\"after_tool_contains\":\"ERROR\",\"tool\":\"apply_diff\",\"args\":{{\"file\":\"a.md\",\"diff\":\"{}\"}}}}\n",
                "{{\"after_tool_contains\":\"applied\",\"tool\":\"bash\",\"args\":{{\"command\":\"grep -q 'new content' a.md && echo verified\",\"timeout_ms\":5000}}}}\n",
                "{{\"after_tool_contains\":\"verified\",\"text\":\"done despite the error\"}}\n",
            ),
            diff.replace('"', "\\\"").replace('\n', "\\n")
        ),
    )
    .unwrap();

    let out = Command::new(bin)
        .args([
            "--plan",
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "help me change the file",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("done despite the error"), "{text}");

    // the failing read is recorded as an error but the task still completed
    let now = std::fs::read_to_string(dir.join("a.md")).unwrap();
    assert!(now.contains("new content"), "patched after a failed read: {now:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
