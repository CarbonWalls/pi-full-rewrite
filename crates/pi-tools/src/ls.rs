//! List a directory.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;

use crate::Toolset;

pub struct ListDir {
    pub root: PathBuf,
}

impl Tool for ListDir {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_dir".into(),
            description:
                "List the entries of a directory (dirs end with `/`), one per line,
alphabetically. Path is relative to the working directory; empty means the root."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "path":{"type":"string","description":"Directory to list (default: working directory)"}
                }
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let ts = Toolset::new(&self.root);
        let pb = match ts.resolve(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rd = match std::fs::read_dir(&pb) {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("list_dir: {e}")),
        };
        let mut entries: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(if is_dir {
                format!("{name}/")
            } else {
                name
            });
        }
        entries.sort();
        if entries.is_empty() {
            return ToolResult::ok(format!("(empty directory: {path})"));
        }
        ToolResult::ok(entries.join("\n"))
    }
}
