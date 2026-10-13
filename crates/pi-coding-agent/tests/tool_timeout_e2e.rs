//! `--tool-timeout N`: a bash tool that sleeps past N must be killed and
//! reported as an error, and the agent must still complete the task.

use std::process::{Command, Stdio};

#[test]
fn tool_timeout_kills_slow_tools_end_to_end() {
    let dir = std::env::temp_dir().join(format!("pi-to-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        "{\"contains\":\"hang please\",\"tool\":\"bash\",\"args\":{\"command\":\"sleep 20\",\"description\":\"hang\"}}\n\
         {\"after_tool_contains\":\"ERROR\",\"tool\":\"bash\",\"args\":{\"command\":\"echo recovered\"}}\n\
         {\"after_tool_contains\":\"recovered\",\"text\":\"timeout handled\"}\n",
    )
    .unwrap();

    let t0 = std::time::Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "-m",
            "mock/echo",
            "--tool-timeout",
            "800",
            "hang please",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let wall = t0.elapsed().as_millis();
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("timeout handled"), "agent recovered after the timeout: {t}");
    assert!(
        wall < 10_000,
        "the run must not wait for `sleep 20` ({wall}ms)"
    );

    // the timed-out tool call is recorded as an error in the session
    let sess = std::fs::read_dir(dir.join(".pi")).unwrap().next().unwrap().unwrap().path();
    let s = std::fs::read_to_string(&sess).unwrap();
    assert!(s.contains("error") || s.contains("timeout"), "error recorded: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}
