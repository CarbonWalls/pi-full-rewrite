//! `--plan`: the plan directive is prepended, the agent drives the todo tool
//! (scenario mock), and the turn completes cleanly.

use std::process::{Command, Stdio};

#[test]
fn plan_flag_drives_todo() {
    let bin = env!("CARGO_BIN_EXE_pi");
    let dir = std::env::temp_dir().join(format!("pi-plan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sd = dir.join(".pi/sessions");
    std::fs::create_dir_all(&sd).unwrap();
    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        concat!(
            "{\"contains\":\"Plan first\",\"tool\":\"todo\",\"args\":{\"todos\":[{\"content\":\"step a\",\"status\":\"pending\"},{\"content\":\"step b\",\"status\":\"in_progress\"}]}}\n",
            "{\"after_tool_contains\":\"step\",\"text\":\"planned and working\"}\n",
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
            sd.to_str().unwrap(),
            "build a feature",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run pi --plan");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));

    // the session must show: plan directive, todo call, tool result, final text
    let sess = std::fs::read_dir(&sd)
        .unwrap()
        .next()
        .expect("session file")
        .unwrap()
        .path();
    let text = std::fs::read_to_string(&sess).unwrap();
    assert!(text.contains("Plan first"), "directive present");
    assert!(text.contains("\"name\":\"todo\""), "todo tool called: {text}");
    assert!(text.contains("TODO updated"), "todo result: {text}");

    let _ = std::fs::remove_dir_all(&dir);
}
