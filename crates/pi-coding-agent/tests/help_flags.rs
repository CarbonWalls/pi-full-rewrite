//! Docs/CLI contract: every flag documented in docs/usage.md and the README
//! must be exposed by `pi --help`. Guards against doc drift.

use std::process::Command;

#[test]
fn documented_flags_exist_in_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .arg("--help")
        .output()
        .expect("pi --help");
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout).into_owned();

    // flags referenced in docs/usage.md
    let documented = [
        "--mode", "--json", "--quiet", "--file", "--template", "--set",
        "--model", "-m", "--fallback", "--list-models", "--info",
        "--root", "--session-dir", "--no-session", "--sessions", "--gc-sessions",
        "--stats", "--tree", "--resume", "--continue", "--export", "--format",
        "--tool-timeout", "--max-output", "--plan", "-P", "--rpc", "--mcp-server",
        "--serve", "--serve-addr", "--serve-token", "--port", "--ext",
        "--mcp", "--review", "--doctor", "--init", "--theme", "--completion",
        "--image", "--quiet", "--man", "--access-log", "--install-package", "--login", "--logout", "--export-all", "--grep-sessions", "--cost"
    ];
    let mut missing: Vec<&str> = Vec::new();
    for f in documented {
        let needle = if f.starts_with("-m") && f == "-m" {
            "-m, --model" // short flag is folded into --model's help line
        } else {
            f
        };
        if !f.starts_with("-m") && !help.contains(needle) {
            missing.push(f);
        }
        if f == "-m" && !help.contains("--model") {
            missing.push(f);
        }
    }
    assert!(
        missing.is_empty(),
        "documented flags missing from --help: {missing:?}\n---\n{help}"
    );
}
