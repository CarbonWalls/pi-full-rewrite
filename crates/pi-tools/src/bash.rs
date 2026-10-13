//! Run a shell command with a timeout, confined to the working directory.
//!
//! stdout/stderr are drained by background reader threads while the child
//! runs (prevents pipe-buffer deadlock on large output); output is capped.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CAP: usize = 20_000;

pub struct Bash {
    pub cwd: PathBuf,
    pub timeout_ms: u64,
    #[allow(dead_code)]
    pub allow_net: bool,
}

impl Tool for Bash {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bash".into(),
            description:
                "Run a shell command in the working directory and return combined
stdout+stderr and the exit code. Commands are run with a timeout (killed on
expiry). Use for builds, tests, git, and other CLI tasks. Avoid long-running
daemons; this is a synchronous command runner. NOTE: bash is a HOST-LEVEL
tool --root does NOT confine it (a coding agent needs the shell); the
file/glob/grep/git tools are confined to --root."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "command":{"type":"string","description":"Shell command line (run via `sh -c`)"},
                    "timeout_ms":{"type":"integer","minimum":1,"description":"Per-command timeout override in ms"}
                },
                "required":["command"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if cmd.is_empty() {
            return ToolResult::err("bash: missing `command`");
        }
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.timeout_ms)
            .clamp(100, 3_600_000);
        let timeout = Duration::from_millis(timeout_ms);

        let mut builder = Command::new("sh");
        builder
            .arg("-c")
            .arg(cmd)
            .current_dir(&self.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // put the whole command tree in its own process group so a timeout can
        // reap grandchildren too (no orphaned background children)
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            builder.process_group(0);
        }
        let mut child = match builder.spawn() {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("bash: failed to spawn: {e}")),
        };

        let (so, se) = (child.stdout.take(), child.stderr.take());
        let (otx, orx) = std::sync::mpsc::channel::<String>();
        let (etx, erx) = std::sync::mpsc::channel::<String>();
        let to = thread::spawn(move || {
            otx.send(drain(so)).ok();
        });
        let te = thread::spawn(move || {
            etx.send(drain(se)).ok();
        });

        let deadline = Instant::now() + timeout;
        let outcome = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        #[cfg(unix)]
                        {
                            // Order matters: killing the shell first would
                            // reparent its children to init and strand them
                            // (group-kill misses job-control sub-groups, and
                            // a reparented child no longer has this shell as
                            // an ancestor). Reap the tree FIRST while the
                            // parentage is intact, then the group, then the
                            // direct child.
                            let _ = reap_process_tree(child.id());
                            let pgid = child.id() as i32;
                            let _ = std::process::Command::new("kill")
                                .args(["-KILL", &format!("-{pgid}")])
                                .status();
                            let _ = child.kill();
                        }
                        let _ = child.wait();
                        break Err(format!(
                            "$ {cmd}\n[timeout after {timeout_ms} ms; process
killed]"
                        ));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(e) => break Err(format!("bash: {e}")),
            }
        };

        // Reader threads finish once the pipes close (child exited/killed).
        // Bounded so a grandchild that survived the group-kill (e.g. after a
        // setsid) cannot hang the agent: abandon the readers after ~2.5 s.
        let out = recv_bounded(&orx, 2500);
        let err = recv_bounded(&erx, 2500);
        // drop handles instead of joining: in the pathological case a leaked
        // grandchild keeps the reader thread alive; we prefer the thread to
        // leak over the agent hanging.
        drop(to);
        drop(te);

        match outcome {
            Ok(status) => {
                let combined = combine(out, err);
                if status.success() {
                    ToolResult::ok(format!("$ {cmd}\n{}", truncate(&combined)))
                } else {
                    ToolResult::err(format!(
                        "$ {cmd}\n{}\n[exit {}]",
                        truncate(&combined),
                        code_of(&status)
                    ))
                }
            }
            Err(msg) => ToolResult::err(msg),
        }
    }
}

fn drain<R: std::io::Read>(rd: Option<R>) -> String {
    let mut b = String::new();
    if let Some(mut r) = rd {
        let mut buf = [0u8; 8192];
        loop {
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if b.len() < CAP {
                        let take = n.min(CAP - b.len());
                        b.push_str(&String::from_utf8_lossy(&buf[..take]));
                    }
                    // past CAP: keep reading (and discarding) so the child
                    // never hits SIGPIPE from a closed pipe (audit finding).
                }
                Err(_) => break,
            }
        }
    }
    b
}

/// Receive from a reader channel with a hard budget, so a runaway child that
/// outlives a failed group-kill cannot hang the whole agent forever.
fn recv_bounded(rx: &std::sync::mpsc::Receiver<String>, budget_ms: u64) -> String {
    use std::sync::mpsc::RecvTimeoutError;
    let deadline = Instant::now() + std::time::Duration::from_millis(budget_ms);
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(v) => return v,
            Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
            Err(_) => return String::new(),
        }
    }
}

fn combine(o: String, e: String) -> String {
    let mut s = String::new();
    if !o.is_empty() {
        s.push_str(&o);
        if !e.is_empty() {
            s.push('\n');
        }
    }
    if !e.is_empty() {
        s.push_str("[stderr]\n");
        s.push_str(&e);
    }
    s
}

fn truncate(s: &str) -> String {
    if s.len() <= CAP {
        s.to_string()
    } else {
        let mut end = CAP;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n…(truncated, kept first {CAP} of {} chars)", &s[..end], s.len())
    }
}

fn code_of(status: &std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}


/// Kill every process in the subtree rooted at `root` (walked via /proc,
/// parent chains — NOT process groups). A job-control shell (`set -m`,
/// `bash -c` with monitor on) or an explicit `setsid` can put grandchildren
/// in their own process group; the group-kill alone would miss them, so the
/// timeout path also reaps by ancestry. Returns how many extra processes
/// were signalled.
pub fn reap_process_tree(root: u32) -> usize {
    use std::collections::HashMap;
    fn parent_map() -> HashMap<u32, u32> {
        let mut map = HashMap::new();
        if let Ok(rd) = std::fs::read_dir("/proc") {
            for e in rd.flatten() {
                let pid: u32 = match e.file_name().to_str().and_then(|s| s.parse().ok()) {
                    Some(p) => p,
                    None => continue,
                };
                if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                    // "pid (comm) state ppid ..." — parse the tail after ')'
                    if let Some(rest) = stat.split(')').nth(1) {
                        let f: Vec<&str> = rest.split_whitespace().collect();
                        if let Some(pp) = f.get(1).and_then(|s| s.parse().ok()) {
                            map.insert(pid, pp);
                        }
                    }
                }
            }
        }
        map
    }
    // extra net: direct children by parent (independent of /proc parsing)
    let _ = std::process::Command::new("pkill")
        .args(["-9", "-P", &root.to_string()])
        .status();
    let mut killed = 0;
    for _ in 0..2 {
        let parent = parent_map();
        let mut targets: Vec<u32> = Vec::new();
        for &pid in parent.keys() {
            if pid == root {
                continue;
            }
            let mut p = pid;
            for _ in 0..64 {
                p = match parent.get(&p) {
                    Some(&x) => x,
                    None => break,
                };
                if p == root {
                    targets.push(pid);
                    break;
                }
            }
        }
        if targets.is_empty() {
            return killed;
        }
        let args: Vec<String> = ["-KILL".to_string()].into_iter().chain(targets.iter().map(|t| t.to_string())).collect();
        let _ = std::process::Command::new("kill").args(&args).status();
        killed += targets.len();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    killed
}
