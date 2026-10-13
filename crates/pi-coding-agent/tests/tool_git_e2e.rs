//! The git tool driven end to end: inside a real repo, a scenario has the
//! agent call the git tool, receive real output, and finish.

use std::process::{Command, Stdio};

#[test]
fn agent_uses_the_git_tool_in_a_real_repo() {
    let dir = std::env::temp_dir().join(format!("pi-gittool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let git = |args: &[&str]| {
        let o = Command::new("git").args(args).current_dir(&dir).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::write(dir.join("a.txt"), "v1\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);

    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        "{\"contains\":\"look at git\",\"tool\":\"git\",\"args\":{\"args\":\"status -sb\"}}\n\
         {\"after_tool_contains\":\"## main\",\"tool\":\"bash\",\"args\":{\"command\":\"echo ok\"}}\n\
         {\"after_tool_contains\":\"ok\",\"text\":\"git status works\"}\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "-m",
            "mock/echo",
            "look at git status",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("git status works"), "completed: {t}");

    // the git tool call is recorded in the session with its real output
    let sess = std::fs::read_dir(dir.join(".pi")).unwrap().next().unwrap().unwrap().path();
    let s = std::fs::read_to_string(&sess).unwrap();
    assert!(s.contains("\"name\":\"git\""), "git tool recorded");
    let _ = std::fs::remove_dir_all(&dir);
}
