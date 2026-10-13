//! Write (create/overwrite) a text file.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;

use crate::Toolset;

pub struct WriteFile {
    pub root: PathBuf,
}

impl Tool for WriteFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description:
                "Create or overwrite a text file with the given content. Parent directories are
created as needed. Prefer `edit_file` for targeted changes to existing files. Path is
relative to the working directory."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "path":{"type":"string","description":"File path (relative to cwd)"},
                    "content":{"type":"string","description":"Full new content"}
                },
                "required":["path","content"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return ToolResult::err("write_file: missing `path`");
        }
        let ts = Toolset::new(&self.root);
        let pb = match ts.resolve(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(parent) = pb.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult::err(format!("write_file: {e}"));
            }
        }
        let existed = pb.exists();
        match std::fs::write(&pb, content) {
            Ok(()) => {
                let bytes = content.len();
                let lines = content.lines().count();
                let action = if existed { "updated" } else { "created" };
                ToolResult::ok(format!("{action} {path} ({bytes} bytes, {lines} lines)"))
            }
            Err(e) => ToolResult::err(format!("write_file: {e}")),
        }
    }
}
