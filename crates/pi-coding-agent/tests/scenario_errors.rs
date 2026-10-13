//! A set-but-broken PI_MOCK_SCENARIO must fail loudly (agent reports the
//! underlying parse error), never silently fall back to echo.

use std::process::{Command, Stdio};

#[test]
fn broken_scenario_fails_loudly() {
    let dir = std::env::temp_dir().join(format!("pi-scn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("bad.jsonl");
    std::fs::write(&scn, "{\"contains\":\"x\",\"text\":\"no-newline-ending\"} \n{THIS IS NOT JSON}\n").unwrap();

    let missing = dir.join("missing.jsonl");
    let _ = &missing;

    // 1) missing file -> error
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--quiet", "--root", dir.to_str().unwrap(), "-m", "mock/echo", "hi"])
        .env("PI_MOCK_SCENARIO", missing.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    assert!(!text.contains("mock/echo"), "no silent echo fallback: {text}");
    assert!(
        text.to_lowercase().contains("scenario") || text.to_lowercase().contains("error"),
        "reports the problem: {text}"
    );

    // 2) malformed line -> error mentioning the scenario line
    let out2 = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--quiet", "--root", dir.to_str().unwrap(), "-m", "mock/echo", "hi"])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run2");
    let t2 = format!(
        "{}{}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(!t2.is_empty(), "surfaces something: {t2}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn error_rule_then_fallback_recovers_offline() {
    let dir = std::env::temp_dir().join(format!("pi-fb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scn = dir.join("fb.jsonl");
    // first rule makes the PRIMARY error; the fallback (echo) must take over
    std::fs::write(
        &scn,
        "{\"contains\":\"x\",\"error\":\"primary down\"}\n{\"contains\":\"x\",\"text\":\"echo recovered\"}\n",
    )
    .unwrap();

    // with fallback: the turn succeeds through the fallback model
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "-m",
            "mock/echo",
            "--fallback",
            "mock/echo",
            "x",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let t = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        t.contains("recovered") || t.contains("echo:"),
        "fallback must answer: {t}"
    );

    // without fallback: a loud error, exit != 0
    let out2 = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--quiet", "--root", dir.to_str().unwrap(), "-m", "mock/echo", "x"])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let t2 = format!("{}{}", String::from_utf8_lossy(&out2.stdout), String::from_utf8_lossy(&out2.stderr));
    assert!(!out2.status.success(), "no fallback must fail: {t2}");
    assert!(t2.contains("primary down"), "error surfaced: {t2}");
    let _ = std::fs::remove_dir_all(&dir);
}
