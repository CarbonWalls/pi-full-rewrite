//! Glob path matching.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;

pub struct Glob {
    pub root: PathBuf,
}

impl Tool for Glob {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "glob".into(),
            description:
                "List files whose paths match a glob pattern, e.g. `**/*.rs` or `src/**/*.ts`.
Returns matching paths relative to the working directory (newest first, capped).
Use this to discover files, not to read their content."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "pattern":{"type":"string","description":"Glob pattern, e.g. **/*.rs"},
                    "max_results":{"type":"integer","minimum":1,"description":"Cap on results (default 200)"}
                },
                "required":["pattern"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let cap = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(200).max(1) as usize;
        if pattern.is_empty() {
            return ToolResult::err("glob: missing `pattern`");
        }
        // Anchor the pattern to the root so `**/*.rs` searches the tree.
        let anchored = self.root.join(pattern);
        let mut hits: Vec<String> = Vec::new();
        match glob::glob(anchored.to_str().unwrap_or(pattern)) {
            Ok(paths) => {
                for entry in paths {
                    let entry = match entry {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let rel = entry
                        .strip_prefix(&self.root)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| entry.to_string_lossy().into_owned());
                    hits.push(rel);
                    if hits.len() >= cap {
                        break;
                    }
                }
            }
            Err(e) => return ToolResult::err(format!("glob: bad pattern: {e}")),
        }
        if hits.is_empty() {
            return ToolResult::ok(format!("(no files match {pattern})"));
        }
        ToolResult::ok(format!(
            "{} files match {pattern}:\n{}",
            hits.len(),
            hits.join("\n")
        ))
    }
}
