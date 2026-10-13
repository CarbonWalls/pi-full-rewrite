//! pi-tools integration tests: run real tools against a temp dir.

use pi_agent::tools::ToolRegistry;
use pi_tools::Toolset;
use serde_json::json;

fn setup(name: &str) -> (Toolset, ToolRegistry, std::path::PathBuf) {
    // Unique per test: tests in one binary run in parallel under the same PID.
    let dir = std::env::temp_dir().join(format!("pi-tools-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/hello.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
    std::fs::write(dir.join("README.md"), "# Test\nreadme body\n").unwrap();
    let ts = Toolset::new(&dir);
    let mut reg = ToolRegistry::new();
    ts.register_all(&mut reg);
    (ts, reg, dir)
}

#[test]
fn read_write_edit_roundtrip() {
    let (_ts, reg, dir) = setup("rw");

    let r = reg.execute_named("read_file", json!({"path":"src/hello.rs"}));
    assert!(!r.is_error, "{:?}", r.output);
    assert!(r.output.contains("fn main"));

    let w = reg.execute_named(
        "write_file",
        json!({"path":"src/new.txt","content":"line1\nline2\n"}),
    );
    assert!(!w.is_error, "{:?}", w.output);
    assert!(dir.join("src/new.txt").exists());

    let e = reg.execute_named(
        "edit_file",
        json!({"path":"src/new.txt","old_string":"line2","new_string":"LINE2"}),
    );
    assert!(!e.is_error, "{:?}", e.output);
    let back = std::fs::read_to_string(dir.join("src/new.txt")).unwrap();
    assert!(back.contains("LINE2"));

    // replace_all on duplicates
    let d2 = reg.execute_named(
        "edit_file",
        json!({"path":"src/new.txt","old_string":"l","new_string":"x","replace_all":true}),
    );
    assert!(!d2.is_error, "{:?}", d2.output);

    // ambiguous match without replace_all fails
    let _w2 = reg.execute_named(
        "write_file",
        json!({"path":"amb.txt","content":"abc abc\n"}),
    );
    let amb = reg.execute_named(
        "edit_file",
        json!({"path":"amb.txt","old_string":"abc","new_string":"z"}),
    );
    assert!(amb.is_error, "duplicate match must fail: {:?}", amb.output);
    let amb2 = reg.execute_named(
        "edit_file",
        json!({"path":"amb.txt","old_string":"abc","new_string":"z","replace_all":true}),
    );
    assert!(!amb2.is_error, "{:?}", amb2.output);

    // path escape blocked
    let esc = reg.execute_named("read_file", json!({"path":"../../etc/passwd"}));
    assert!(esc.is_error, "{:?}", esc.output);
}

#[test]
fn glob_and_grep() {
    let (_ts, reg, _dir) = setup("glob");
    let g = reg.execute_named("glob", json!({"pattern":"**/*.rs"}));
    assert!(!g.is_error, "{:?}", g.output);
    assert!(g.output.contains("hello.rs"), "{}", g.output);

    let gr = reg.execute_named("grep", json!({"pattern":"println"}));
    assert!(!gr.is_error, "{:?}", gr.output);
    assert!(gr.output.contains("hello.rs"), "{}", gr.output);
}

#[test]
fn ls_lists_entries() {
    let (_ts, reg, _dir) = setup("ls");
    let l = reg.execute_named("list_dir", json!({"path":"src"}));
    assert!(!l.is_error, "{:?}", l.output);
    assert!(l.output.contains("hello.rs"), "{}", l.output);
}

#[test]
fn bash_runs_and_reports_exit() {
    let (_ts, reg, _dir) = setup("bash");
    let ok = reg.execute_named("bash", json!({"command":"echo hello-pi"}));
    assert!(!ok.is_error, "{:?}", ok.output);
    assert!(ok.output.contains("hello-pi"), "{}", ok.output);

    let bad = reg.execute_named("bash", json!({"command":"exit 3"}));
    assert!(bad.is_error, "{:?}", bad.output);
    assert!(bad.output.contains("[exit 3]"), "{}", bad.output);

    let to = reg.execute_named("bash", json!({"command":"sleep 5","timeout_ms":300}));
    assert!(to.is_error, "timeout should be an error");
    assert!(to.output.contains("timeout"), "{}", to.output);
}

#[test]
fn todo_updates_list() {
    let (_ts, reg, _dir) = setup("todo");
    let t = reg.execute_named(
        "todo",
        json!({"todos":[
            {"content":"step one","status":"done"},
            {"content":"step two","status":"in_progress"},
            {"content":"step three","status":"pending"}
        ]}),
    );
    assert!(!t.is_error, "{:?}", t.output);
    assert!(t.output.contains("1/3 done"), "{}", t.output);
}


#[test]
fn git_tool_readonly() {
    let (_ts, reg, dir) = setup("git");
    // init a repo in the sandbox dir
    let out = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .expect("git init");
    assert!(out.status.success());
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();

    let r = reg.execute_named("git", serde_json::json!({"args":"status --short"}));
    assert!(!r.is_error, "{}", r.output);
    assert!(r.output.contains("a.txt"), "{}", r.output);

    // untracked -> diff is empty; commit then modify for a real diff
    let _ = std::process::Command::new("git")
        .args(["add", "a.txt"])
        .current_dir(&dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "-qm", "init"])
        .current_dir(&dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output();
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    let d = reg.execute_named("git", serde_json::json!({"args":"diff"}));
    assert!(!d.is_error);
    assert!(d.output.contains("+changed"), "{}", d.output);

    // disallowed subcommand
    let bad = reg.execute_named("git", serde_json::json!({"args":"push origin"}));
    assert!(bad.is_error, "push must be rejected");
    // escape flag rejected
    let esc = reg.execute_named("git", serde_json::json!({"args":"diff -C /etc"}));
    assert!(esc.is_error, "-C must be rejected");
}

#[test]
fn bg_bash_lifecycle() {
    let (_ts, reg, _dir) = setup("bg");
    // start a background job that prints and sleeps
    let s = reg.execute_named("bg_bash", serde_json::json!({"command":"echo bg-hello; sleep 0.2"}));
    assert!(!s.is_error, "{}", s.output);
    let id: String = s.output.split([' ', ':']).next().unwrap_or("").to_string();
    assert!(!id.is_empty(), "no id in: {}", s.output);

    // poll until done
    let mut got = String::new();
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let r = reg.execute_named("bg_read", serde_json::json!({"id": id}));
        assert!(!r.is_error, "{}", r.output);
        got = r.output.clone();
        let done: bool = got.contains("\"done\":true");
        if done {
            break;
        }
    }
    assert!(got.contains("bg-hello"), "{}", got);
    assert!(got.contains("\"done\":true"), "job should finish: {got}");

    // list shows it
    let l = reg.execute_named("bg_list", serde_json::json!({}));
    assert!(!l.is_error);
    assert!(l.output.contains(&id), "{}", l.output);

    // killing an unknown job errors
    let k = reg.execute_named("bg_kill", serde_json::json!({"id":"bg_nope"}));
    assert!(k.is_error);
}

#[test]
fn fs_ops_sandboxed() {
    let (_ts, reg, dir) = setup("fs");
    std::fs::write(dir.join("a.txt"), "hi").unwrap();
    // mkdir
    let m = reg.execute_named("fs", serde_json::json!({"op":"mkdir","path":"sub/inner"}));
    assert!(!m.is_error, "{}", m.output);
    assert!(dir.join("sub/inner").is_dir());
    // move
    let mv = reg.execute_named("fs", serde_json::json!({"op":"move","src":"a.txt","dst":"sub/inner/b.txt"}));
    assert!(!mv.is_error, "{}", mv.output);
    assert!(!dir.join("a.txt").exists() && dir.join("sub/inner/b.txt").exists());
    // copy dir
    let cp = reg.execute_named("fs", serde_json::json!({"op":"copy","src":"sub","dst":"sub2"}));
    assert!(!cp.is_error, "{}", cp.output);
    assert!(dir.join("sub2/inner/b.txt").exists());
    // remove file
    let rm = reg.execute_named("fs", serde_json::json!({"op":"remove","path":"sub/inner/b.txt"}));
    assert!(!rm.is_error, "{}", rm.output);
    // remove dir requires recursive
    let rm2 = reg.execute_named("fs", serde_json::json!({"op":"remove","path":"sub2"}));
    assert!(rm2.is_error, "dir remove must require recursive");
    let rm3 = reg.execute_named("fs", serde_json::json!({"op":"remove","path":"sub2","recursive":true}));
    assert!(!rm3.is_error, "{}", rm3.output);
    // escape blocked
    let esc = reg.execute_named("fs", serde_json::json!({"op":"remove","path":"../../etc/hosts"}));
    assert!(esc.is_error);
}

#[test]
fn apply_diff_applies_git_diff() {
    let (_ts, reg, dir) = setup("applydiff");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git")
            .status
            .success()
    };
    assert!(git(&["init", "-q"]));
    // hermetic identity (CI runners ship none)
    assert!(git(&["config", "user.name", "ci test"]));
    assert!(git(&["config", "user.email", "ci@example.invalid"]));
    std::fs::write(dir.join("f.txt"), "alpha\nbeta\ngamma\n").unwrap();
    assert!(git(&["add", "f.txt"]));
    assert!(git(&["commit", "-qm", "init"]));
    std::fs::write(dir.join("f.txt"), "alpha\nBETA\ngamma\ndelta\n").unwrap();
    let out = std::process::Command::new("git")
        .args(["diff"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let diff = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(diff.contains("@@"), "{diff}");

    // restore the OLD content so applying the diff reproduces the change
    std::fs::write(dir.join("f.txt"), "alpha\nbeta\ngamma\n").unwrap();
    let r = reg.execute_named("apply_diff", serde_json::json!({"file":"f.txt","diff":diff}));
    assert!(!r.is_error, "{}", r.output);
    let content = std::fs::read_to_string(dir.join("f.txt")).unwrap();
    assert_eq!(content, "alpha\nBETA\ngamma\ndelta\n");
}

#[test]
fn bash_timeout_kills_process_group() {
    let (_ts, reg, dir) = setup("bgt");
    // spawn a background grandchild that writes its pid, then wait forever
    let cmd = "echo $$ > leader.pid; sleep 30 & echo $! > grand.pid; wait";
    let r = reg.execute_named(
        "bash",
        serde_json::json!({"command": cmd, "timeout_ms": 400}),
    );
    assert!(r.is_error, "should time out");
    assert!(r.output.contains("timeout"), "{}", r.output);
    // the grandchild pid file was written before the kill
    let grand: u32 = std::fs::read_to_string(dir.join("grand.pid"))
        .unwrap()
        .trim()
        .parse()
        .expect("grandchild pid file");
    let leader: u32 = std::fs::read_to_string(dir.join("leader.pid"))
        .unwrap()
        .trim()
        .parse()
        .expect("leader pid file");
    // give the kernels a moment, then both must be gone (loaded CI runners
    // reap /proc entries slower than a dev box; and a recycled pid must not
    // fake the grandchild as alive)
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let helper = |pid: u32| -> (bool, String) {
        match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            Ok(c) => (true, c.trim().to_string()),
            Err(_) => (false, String::new()),
        }
    };
    let (galive, gcomm) = helper(grand);
    assert!(!galive || gcomm != "sleep", "grandchild {grand} must be killed (comm={gcomm:?})");
    let (lalive, _) = helper(leader);
    assert!(!lalive, "leader {leader} must be killed");
}

#[test]
fn paths_cannot_escape_the_root() {
    let (ts, reg, _dir) = setup("nopath");
    // absolute + parent-dot escapes rejected by read/glob/grep
    for (tool, args) in [
        ("read_file", json!({"path": "/etc/hostname"})),
        ("read_file", json!({"path": "../../../etc/passwd"})),
        ("glob", json!({"pattern": "/etc/**"})),
        ("grep", json!({"pattern": "root", "path": "../../etc"})),
    ] {
        let r = reg.execute_named(tool, args);
        assert!(r.is_error, "{tool} must reject the escape: {:?}", r.output);
    }
    let _ = ts;
}

#[test]
fn symlinks_outside_the_root_are_blocked() {
    let (ts, reg, dir) = setup("symescape");
    // a symlink INSIDE the root pointing outside must not allow reads
    let out = dir.parent().unwrap().join(format!("outside-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("secret.txt"), "s3cr3t").unwrap();
    std::os::unix::fs::symlink(&out, dir.join("evil")).unwrap();

    let r = reg.execute_named("read_file", json!({"path": "evil/secret.txt"}));
    assert!(r.is_error, "symlink escape must be blocked: {:?}", r.output);
    let _ = std::fs::remove_dir_all(&out);
    let _ = ts;
}

#[test]
fn git_output_flag_is_blocked() {
    let (ts, reg, dir) = setup("gitout");
    // init a repo so the git tool has something to diff
    use std::process::Command;
    Command::new("git").args(["init", "-q", "-b", "main"]).current_dir(&dir).output().unwrap();
    Command::new("git").args(["config", "user.email", "t@t"]).current_dir(&dir).output().unwrap();
    Command::new("git").args(["config", "user.name", "t"]).current_dir(&dir).output().unwrap();
    Command::new("git").args(["add", "."]).current_dir(&dir).output().unwrap();
    Command::new("git").args(["commit", "-qm", "init"]).current_dir(&dir).output().unwrap();
    std::fs::write(dir.join("src/hello.rs"), "fn main() {\n    println!(\"X\");\n}\n").unwrap();

    for arg in ["diff --output=/tmp/pi-git-escape -- src/hello.rs", "show --output=/tmp/pi-git-escape HEAD"] {
        let r = reg.execute_named("git", json!({"args": arg}));
        assert!(r.is_error, "git {arg:?} must be blocked: {:?}", r.output);
    }
    assert!(!std::path::Path::new("/tmp/pi-git-escape").exists(), "no file escaped");
    let _ = ts;
}
