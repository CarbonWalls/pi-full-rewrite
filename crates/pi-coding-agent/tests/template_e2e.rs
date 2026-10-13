//! Template system through the binary: a project template with placeholders,
//! filled from built-ins and `--set`, reaching the model prompt.

use std::process::{Command, Stdio};

#[test]
fn template_variables_reach_the_prompt() {
    let dir = std::env::temp_dir().join(format!("pi-tpl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".pi/templates")).unwrap();
    std::fs::write(
        dir.join(".pi/templates/bug.md"),
        "BUG REPORT for {{project}} on {{os}}\n---\nPurpose: {{purpose}}\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--template",
            "bug",
            "--set",
            "project=pi-rs",
            "--set",
            "purpose=reproduce",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi/s").to_str().unwrap(),
            "-m",
            "mock/echo",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("BUG REPORT"), "template body used: {t}");
    assert!(t.contains("for pi-rs"), "--set var substituted: {t}");
    assert!(t.contains("Purpose: reproduce"), "purpose substituted: {t}");
    assert!(t.contains("{{os}}") == false && t.contains("linux"), "built-in {{os}} filled: {t}");
    assert!(!t.contains("{{project}}"), "no leftover placeholders: {t}");
    let _ = std::fs::remove_dir_all(&dir);
}
