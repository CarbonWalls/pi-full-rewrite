//! `fs`: filesystem mutation tool (move / copy / remove / mkdir), sandboxed
//! to the working root. Fills the gap that agents need (they previously could
//! only read/write/edit within files).

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct Fs {
    pub root: PathBuf,
}

impl Fs {
    fn resolve(&self, p: &str) -> Result<PathBuf, String> {
        crate::resolve_under(&self.root, p)
    }

    fn remove(&self, p: &PathBuf, recursive: bool) -> Result<String, String> {
        let meta = std::fs::symlink_metadata(p).map_err(|e| format!("remove: {e}"))?;
        if meta.is_dir() && !recursive {
            return Err("fs: target is a directory; set recursive=true to remove".into());
        }
        if meta.is_dir() {
            std::fs::remove_dir_all(p).map_err(|e| format!("remove: {e}"))?;
        } else {
            std::fs::remove_file(p).map_err(|e| format!("remove: {e}"))?;
        }
        Ok(format!("removed {}", p.display()))
    }
}

impl Tool for Fs {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "fs".into(),
            description: "Filesystem operations within the working directory: move, copy, remove,
mkdir. {op} one of: move|copy|remove|mkdir. move/copy need src+dst; remove needs path
(+recursive=true for directories); mkdir needs path.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "op":{"type":"string","enum":["move","copy","remove","mkdir"]},
                    "src":{"type":"string","description":"source path (move/copy)"},
                    "dst":{"type":"string","description":"destination path (move/copy)"},
                    "path":{"type":"string","description":"target path (remove/mkdir)"},
                    "recursive":{"type":"boolean","description":"recursively remove a directory"}
                },
                "required":["op"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");
        let src = args.get("src").and_then(|v| v.as_str()).unwrap_or("");
        let dst = args.get("dst").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);

        match op {
            "mkdir" => {
                let p = match self.resolve(if path.is_empty() { src } else { path }) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                match std::fs::create_dir_all(&p) {
                    Ok(_) => ToolResult::ok(format!("created {}", p.display())),
                    Err(e) => ToolResult::err(format!("mkdir: {e}")),
                }
            }
            "remove" => {
                let p = match self.resolve(if path.is_empty() { dst } else { path }) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                match self.remove(&p, recursive) {
                    Ok(s) => ToolResult::ok(s),
                    Err(e) => ToolResult::err(e),
                }
            }
            "move" => {
                let a = match self.resolve(src) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                let b = match self.resolve(dst) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                if !a.exists() {
                    return ToolResult::err(format!("fs: source not found: {src}"));
                }
                if let Some(parent) = b.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::rename(&a, &b) {
                    Ok(_) => ToolResult::ok(format!("moved {} -> {}", a.display(), b.display())),
                    Err(e) => ToolResult::err(format!("move: {e}")),
                }
            }
            "copy" => {
                let a = match self.resolve(src) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                let b = match self.resolve(dst) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(e),
                };
                if !a.exists() {
                    return ToolResult::err(format!("fs: source not found: {src}"));
                }
                if let Some(parent) = b.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let res = if a.is_dir() {
                    copy_dir(&a, &b)
                } else {
                    std::fs::copy(&a, &b).map(|_| ())
                };
                match res {
                    Ok(_) => ToolResult::ok(format!("copied {} -> {}", a.display(), b.display())),
                    Err(e) => ToolResult::err(format!("copy: {e}")),
                }
            }
            other => ToolResult::err(format!("fs: unknown op '{other}' (move|copy|remove|mkdir)")),
        }
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

