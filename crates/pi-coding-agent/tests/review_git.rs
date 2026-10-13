//! git-backed `--review`: a real repo with a staged change must feed the
//! diff into the review prompt; a non-repo must stay bounded (collapsed git
//! usage help, not a flood).

use std::process::{Command, Stdio};

fn git(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn review_feeds_the_real_diff() {
    let dir = std::env::temp_dir().join(format!("pi-rev-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "init"]);

    // an unstaged change + a staged change
    std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(dir.join("b.txt"), "brand new\n").unwrap();
    git(&dir, &["add", "b.txt"]);

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--review",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi/r").to_str().unwrap(),
            "-m",
            "mock/echo",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run review");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("+three"), "working diff reached the prompt: {t}");
    assert!(t.contains("b.txt"), "staged/untracked shown: {t}");
    assert!(t.contains("GIT STATUS") || t.contains("git status"), "status section");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn review_on_a_non_repo_stays_bounded() {
    let dir = std::env::temp_dir().join(format!("pi-revnav-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("x.txt"), "hello\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--review",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi/r").to_str().unwrap(),
            "-m",
            "mock/echo",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run review");
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    // the collapsed marker (or an explicit failure note), NOT thousands of help lines
    assert!(
        t.contains("git help truncated") || t.contains("not a git repository") || t.contains("(git failed"),
        "non-repo review stays bounded: {}",
        &t[..t.len().min(300)]
    );
    assert!(!t.contains("usage: git "), "no raw usage flood");
    let _ = std::fs::remove_dir_all(&dir);
}
