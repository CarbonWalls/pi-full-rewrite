//! Targeted string-replace edit on an existing file.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::path::PathBuf;

use crate::Toolset;

pub struct EditFile {
    pub root: PathBuf,
}

impl Tool for EditFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "edit_file".into(),
            description:
                "Replace an exact, literal substring in a file with new text. `old_string`
must match exactly (including whitespace/indentation). If it is not unique,
`replace_all=true` replaces every occurrence, otherwise the edit fails. Read the
file first so you copy the exact text. Path is relative to the working directory."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "path":{"type":"string","description":"File path (relative to cwd)"},
                    "old_string":{"type":"string","description":"Exact text to find (literal, not regex)"},
                    "new_string":{"type":"string","description":"Replacement text (empty to delete)"},
                    "replace_all":{"type":"boolean","description":"Replace all occurrences (default false)"}
                },
                "required":["path","old_string","new_string"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
        let new = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
        let all = args.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);
        if path.is_empty() || old.is_empty() {
            return ToolResult::err("edit_file: `path` and non-empty `old_string` required");
        }
        let ts = Toolset::new(&self.root);
        let pb = match ts.resolve(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let data = match std::fs::read_to_string(&pb) {
            Ok(d) => d,
            Err(e) => return ToolResult::err(format!("edit_file: {e}")),
        };
        let count = data.matches(old).count();
        if count == 0 {
            return ToolResult::err(format!(
                "edit_file: `old_string` not found in {path}; read the file and copy it exactly"
            ));
        }
        if count > 1 && !all {
            return ToolResult::err(format!(
                "edit_file: `old_string` matches {count} times in {path}; make it unique or set
replace_all=true"
            ));
        }
        let updated = if all {
            data.replace(old, new)
        } else {
            data.replacen(old, new, 1)
        };
        let n = if all { count } else { 1 };
        match std::fs::write(&pb, updated) {
            Ok(()) => ToolResult::ok(format!("edited {path} ({n} replacement{n})")),
            Err(e) => ToolResult::err(format!("edit_file: {e}")),
        }
    }
}
