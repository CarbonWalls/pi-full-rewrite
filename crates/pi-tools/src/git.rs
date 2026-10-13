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
        }) {
            return ToolResult::err("git: -C / -c / --git-dir / --work-tree flags are not allowed");
        }
        let argv: Vec<String> = std::iter::once("git".to_string())
            .chain(words.iter().map(|s| s.to_string()))
            .collect();
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let out = match output {
            Ok(o) => o,
            Err(e) => return ToolResult::err(format!("git: spawn failed: {e}")),
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
