//! `pi --cost` aggregates the per-turn Usage entries pi records in sessions.

use std::process::{Command, Stdio};

#[test]
fn cost_aggregates_all_sessions() {
    let dir = std::env::temp_dir().join(format!("pi-cost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".pi")).unwrap();
    let bin = env!("CARGO_BIN_EXE_pi");

    for i in 0..2 {
        let out = Command::new(bin)
            .args([
                "--quiet",
                "--root",
                dir.to_str().unwrap(),
                "--session-dir",
                dir.join(".pi").to_str().unwrap(),
                "-m",
                "mock/echo",
                &format!("turn {i}"),
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(out.status.success());
    }

    let out = Command::new(bin)
        .args([
            "--cost",
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("2 model calls"), "{t}");
    assert!(t.contains("in=") && t.contains("out=") && t.contains("cost="), "{t}");
    assert!(t.contains("mock/echo"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}
