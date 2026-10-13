//! Regex content search across files (ripgrep-lite).

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use regex::Regex;
use serde_json::Value;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct Grep {
    pub root: PathBuf,
}

impl Tool for Grep {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "grep".into(),
            description:
                "Search file contents with a regular expression and return matching lines as
`path:line: text`. By default skips `target`, `.git`, `node_modules`, and binary
files; cap on results is applied. Use to locate where symbols/strings appear."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "pattern":{"type":"string","description":"Rust regex pattern"},
                    "path":{"type":"string","description":"Optional subdirectory to restrict the search (default: whole tree)"},
                    "glob":{"type":"string","description":"Optional path filter, e.g. '*.rs'"},
                    "max_results":{"type":"integer","minimum":1,"description":"Cap on matching lines (default 200)"}
                },
                "required":["pattern"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let sub = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let globf = args.get("glob").and_then(|v| v.as_str());
        let cap = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(200).max(1) as usize;
        if pattern.is_empty() {
            return ToolResult::err("grep: missing `pattern`");
        }
        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("grep: invalid regex: {e}")),
        };
        let base = if sub.is_empty() {
            self.root.clone()
        } else {
            match crate::resolve_under(&self.root, sub) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(format!("grep: {e}")),
            }
        };
        if !base.exists() {
            return ToolResult::err(format!("grep: directory not found: {sub}"));
        }
        let mut matches_out: Vec<String> = Vec::new();
        let mut files_searched = 0usize;
        'walk: for entry in WalkDir::new(&base).into_iter() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue; // skip dotfiles
            }
            if is_skippable(&entry.path().to_string_lossy()) {
                continue;
            }
            if let Some(g) = globf {
                if !match_glob(g, &name) {
                    continue;
                }
            }
            let data = match std::fs::read_to_string(entry.path()) {
                Ok(d) => d,
                Err(_) => continue, // binary / unreadable
            };
            files_searched += 1;
            let rel = entry
                .path()
                .strip_prefix(&self.root)
                .unwrap_or(entry.path())
                .display()
                .to_string();
            for (i, line) in data.lines().enumerate() {
                if re.is_match(line) {
                    matches_out.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
                    if matches_out.len() >= cap {
                        break 'walk;
                    }
                }
            }
        }
        if matches_out.is_empty() {
            return ToolResult::ok(format!(
                "(no matches for `{pattern}` in {files_searched} files)"
            ));
        }
        ToolResult::ok(matches_out.join("\n"))
    }
}

fn is_skippable(path: &str) -> bool {
    const DIRS: &[&str] = &["/target/", "/.git/", "/node_modules/", "/.cargo-cache/"];
    DIRS.iter().any(|d| path.contains(d))
}

/// Minimal `*`/`?` glob against a file name.
fn match_glob(pat: &str, name: &str) -> bool {
    glob::Pattern::new(pat)
        .map(|p| p.matches(name))
        .unwrap_or(false)
}
