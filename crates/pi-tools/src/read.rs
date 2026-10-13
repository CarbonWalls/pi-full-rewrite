//! Read a text file with optional line window.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;

use crate::Toolset;

pub struct ReadFile {
    pub root: PathBuf,
}

impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description:
                "Read a UTF-8 text file, returning numbered lines. Supports 1-based `start_line`
and `max_lines` to page through large files. Path is relative to the working
directory."
                    .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type":"string","description":"File path (relative to cwd)"},
                    "start_line": {"type":"integer","minimum":1,"description":"1-based first line to return (default 1)"},
                    "max_lines": {"type":"integer","minimum":1,"description":"Max lines to return (default 400)"}
                },
                "required":["path"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return ToolResult::err("read_file: missing `path`");
        }
        let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
        let max = args.get("max_lines").and_then(|v| v.as_u64()).unwrap_or(400).max(1) as usize;

        let ts = Toolset::new(&self.root);
        let pb = match ts.resolve(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !pb.exists() {
            return ToolResult::err(format!("read_file: no such file: {path}"));
        }
        if pb.is_dir() {
            return ToolResult::err(format!("read_file: {path} is a directory"));
        }
        let data = match std::fs::read_to_string(&pb) {
            Ok(d) => d,
            Err(e) => return ToolResult::err(format!("read_file: {e}")),
        };
        let lines: Vec<&str> = data.lines().collect();
        let total = lines.len();
        if start > total {
            return ToolResult::ok(format!(
                "(file has {total} lines; start_line {start} is past the end)"
            ));
        }
        let end = (start - 1 + max).min(total);
        let out: Vec<String> = lines[start - 1..end]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:>5}\t{l}", start + i))
            .collect();
        let more = if end < total {
            format!("\n…({} more lines)", total - end)
        } else {
            String::new()
        };
        ToolResult::ok(format!("[{}:{}]\n{}\n{more}", start, end, out.join("\n")))
    }
}
