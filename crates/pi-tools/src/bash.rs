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
                        let _ = child.kill();
                        // reap the whole process group (grandchildren too)
                        #[cfg(unix)]
                        {
                            let pgid = child.id() as i32;
                            let _ = std::process::Command::new("kill")
                                .args(["-KILL", &format!("-{pgid}")])
                                .status();
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

