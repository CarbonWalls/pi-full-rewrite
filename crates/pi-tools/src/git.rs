//! Git inspection tool: whitelisted, shell-free subcommands in the working dir.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAX_OUTPUT: usize = 100_000;

/// Whitelisted read-only git subcommands (no shell, no argument injection).
const ALLOWED: &[&str] = &[
    "diff",
    "status",
    "log",
    "show",
    "rev-parse",
    "branch",
    "ls-files",
    "blame",
];

pub struct Git {
    pub root: PathBuf,
}

impl Tool for Git {
    fn spec(&self) -> ToolSpec {
        let allowed = ALLOWED.join(", ");
        ToolSpec {
            name: "git".into(),
            description: format!(
                "Run a read-only git command in the working directory ({}).
Subcommand must be one of: {}. Extra args are given after a space, e.g.
'git diff --stat' or 'git log -n 5'. Output is truncated to {MAX_OUTPUT} chars.",
                self.root.display(),
                allowed
            )
            .trim()
            .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "args":{"type":"string","description":"'subcommand [extra args]', e.g. \"diff --cached\" or \"log -n 10\""}
                },
                "required":["args"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let spec = match args.get("args").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return ToolResult::err("git: missing 'args' (e.g. \"diff --stat\")"),
        };
        let words: Vec<&str> = spec.split_whitespace().collect();
        let sub = words.first().copied().unwrap_or("");
        if !ALLOWED.contains(&sub) {
            return ToolResult::err(format!(
                "git: subcommand '{sub}' not allowed (read-only: {})",
                ALLOWED.join(", ")
            ));
        }
        // Reject flags that could escape the sandbox (path/override flags).
        if words.iter().any(|w| {
            *w == "-C"
                || *w == "-c"
                || w.starts_with("--git-dir")
                || w.starts_with("--work-tree")
                || w.starts_with("--namespace")
                || w.starts_with("--output") // writes arbitrary files (audit CRITICAL)
                || *w == "-O" || w.starts_with("-O")
        }) {
            return ToolResult::err("git: -C / -c / --git-dir / --work-tree / --output flags are not allowed");
        }
        let argv: Vec<String> = std::iter::once("git".to_string())
            .chain(words.iter().map(|s| s.to_string()))
            .collect();
        let mut child = match Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("git: spawn failed: {e}")),
        };
        // bounded wait: a stuck `git` must not block the agent forever
        const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        let out = {
            let mut out = None;
            let timeout = std::time::Instant::now() + GIT_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(st)) => {
                        let _ = child.wait(); // reap
                        use std::io::Read;
                        let mut o = std::process::Output {
                            status: st,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                        };
                        if let Some(mut so) = child.stdout.take() {
                            let _ = so.read_to_end(&mut o.stdout);
                        }
                        if let Some(mut se) = child.stderr.take() {
                            let _ = se.read_to_end(&mut o.stderr);
                        }
                        out = Some(o);
                        break;
                    }
                    Ok(None) if std::time::Instant::now() >= timeout => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return ToolResult::err("git: timed out after 30 s");
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
                    Err(_) => break,
                }
            }
            match out {
                Some(o) => o,
                None => return ToolResult::err("git: failed to read output"),
            }
        };
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        if !err.trim().is_empty() {
            text.push_str(&format!("\n[stderr] {err}"));
        }
        text.push_str(&format!("\n[exit: {}]", out.status.code().unwrap_or(-1)));
        if text.len() > MAX_OUTPUT {
            let t: String = text.chars().take(MAX_OUTPUT).collect();
            text = format!("{t}\n…(truncated, {MAX_OUTPUT} cap)");
        }
        if out.status.success() || text.trim().is_empty() {
            ToolResult::ok(text)
        } else {
            ToolResult {
                output: text,
                is_error: true,
                meta: None,
            }
        }
    }
}
