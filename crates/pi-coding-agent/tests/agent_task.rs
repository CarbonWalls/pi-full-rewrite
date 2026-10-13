//! Full plan-driven agentic task through the real binary: `--plan` (todo
//! list) -> read the file -> apply a real diff -> verify with bash -> finish.
//! Driven deterministically by PI_MOCK_SCENARIO.

use std::process::{Command, Stdio};

fn esc_diff(d: &str) -> String {
    d.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

#[test]
fn plan_driven_coding_task() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-task-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.py"), "def old():\n    return 1\n").unwrap();

    let diff = "--- a/a.py\n+++ b/a.py\n@@ -1,2 +1,2 @@\n-def old():\n-    return 1\n+def answer():\n+    return 42\n";
    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        format!(
            concat!(
                "{{\"contains\":\"Plan first\",\"tool\":\"todo\",\"args\":{{\"todos\":[{{\"content\":\"read the file\",\"status\":\"in_progress\"}},{{\"content\":\"patch it\",\"status\":\"pending\"}}]}}}}\n",
                "{{\"after_tool_contains\":\"TODO\",\"tool\":\"read_file\",\"args\":{{\"path\":\"a.py\"}}}}\n",
                "{{\"after_tool_contains\":\"def old\",\"tool\":\"apply_diff\",\"args\":{{\"file\":\"a.py\",\"diff\":\"{diff}\"}}}}\n",
                "{{\"after_tool_contains\":\"applied\",\"tool\":\"bash\",\"args\":{{\"command\":\"python3 -c 'import a; assert a.answer() == 42; print(\\\"ok\\\")\'\",\"timeout_ms\":5000}}}}\n",
                "{{\"after_tool_contains\":\"ok\",\"text\":\"task complete\"}}\n",
            ),
            diff = esc_diff(diff)
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
            "refactor the function so answer() returns 42",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run pi");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("task complete"), "finished the task: {text}");

    // the file on disk really changed
    let now = std::fs::read_to_string(dir.join("a.py")).unwrap();
    assert!(now.contains("def answer()"), "patched: {now:?}");
    assert!(now.contains("return 42"), "patched: {now:?}");

    // the session recorded the tool chain: todo -> read_file -> apply_diff -> bash
    let sess = std::fs::read_dir(dir.join(".pi")).unwrap().next().unwrap().unwrap().path();
    let s = std::fs::read_to_string(&sess).unwrap();
    for needle in ["\"name\":\"todo\"", "\"name\":\"read_file\"", "\"name\":\"apply_diff\"", "\"name\":\"bash\""] {
        assert!(s.contains(needle), "session must contain {needle}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
