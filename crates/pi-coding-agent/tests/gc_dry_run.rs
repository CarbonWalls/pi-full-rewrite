//! `--gc-sessions --gc-dry-run` must preview without deleting.

use std::process::{Command, Stdio};

#[test]
fn gc_dry_run_previews_without_deleting() {
    let dir = std::env::temp_dir().join(format!("pi-gcdr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".pi")).unwrap();
    let bin = env!("CARGO_BIN_EXE_pi");
    for i in 0..4 {
        Command::new(bin)
            .args(["--quiet", "--root", dir.to_str().unwrap(), "--session-dir",
                   dir.join(".pi").to_str().unwrap(), "-m", "mock/echo", &format!("t{i}")])
            .stdin(Stdio::null())
            .output().unwrap();
    }
    let before = std::fs::read_dir(dir.join(".pi")).unwrap().count();
    let out = Command::new(bin)
        .args(["--gc-sessions", "2", "--gc-dry-run", "--session-dir", dir.join(".pi").to_str().unwrap()])
        .stdin(Stdio::null())
        .output().unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("would remove 2 file(s)"), "{t}");
    let after = std::fs::read_dir(dir.join(".pi")).unwrap().count();
    assert_eq!(before, after, "dry run must not delete");
    let _ = std::fs::remove_dir_all(&dir);
}
