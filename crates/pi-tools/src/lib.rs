//! Built-in coding tools (read / write / edit / bash / glob / grep / ls /
//! todo) implementing [`pi_agent::tools::Tool`]. All path tools are confined
//! to a working root (sandbox escape blocked).

pub mod bash;
pub mod bg;
pub mod edit;
pub mod fs;
pub mod git;
pub mod glob;
pub mod grep;
pub mod ls;
pub mod patch;
pub mod read;
pub mod todo;
pub mod write;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_agent::tools::ToolRegistry;

/// A tool set confined to a root directory, shared across threads.
#[derive(Clone)]
pub struct Toolset {
    pub root: PathBuf,
    pub todos: Arc<Mutex<Vec<todo::Todo>>>,
    pub bg: Arc<bg::BgRegistry>,
    /// Default timeout for the bash tool, in ms.
    pub tool_timeout_ms: u64,
}

impl Toolset {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            bg: Arc::new(bg::BgRegistry::new(root.join(".pi").join("bg"))),
            todos: Arc::new(Mutex::new(Vec::new())),
            root: root.clone(),
            tool_timeout_ms: 60_000,
        }
    }

    /// Override the default bash tool timeout (ms).
    pub fn with_tool_timeout(mut self, ms: u64) -> Self {
        if ms > 0 {
            self.tool_timeout_ms = ms;
        }
        self
    }

    /// Register the full built-in tool set.
    pub fn register_all(&self, reg: &mut ToolRegistry) {
        reg.register(std::sync::Arc::new(read::ReadFile { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(write::WriteFile { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(edit::EditFile { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(glob::Glob { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(grep::Grep { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(ls::ListDir { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(bash::Bash {
            cwd: self.root.clone(),
            timeout_ms: self.tool_timeout_ms,
            allow_net: false,
        }));
        reg.register(std::sync::Arc::new(todo::TodoTool {
            todos: self.todos.clone(),
        }));
        reg.register(std::sync::Arc::new(git::Git {
            root: self.root.clone(),
        }));
        reg.register(std::sync::Arc::new(bg::BgBash {
            reg: self.bg.clone(),
            cwd: self.root.clone(),
        }));
        reg.register(std::sync::Arc::new(bg::BgRead { reg: self.bg.clone() }));
        reg.register(std::sync::Arc::new(bg::BgKill { reg: self.bg.clone() }));
        reg.register(std::sync::Arc::new(bg::BgList { reg: self.bg.clone() }));
        reg.register(std::sync::Arc::new(fs::Fs { root: self.root.clone() }));
        reg.register(std::sync::Arc::new(patch::ApplyDiff { root: self.root.clone() }));
    }

    /// Resolve `p` under root, rejecting escapes (dot-segments / absolute
    /// paths that land outside).
    pub fn resolve(&self, p: &str) -> Result<PathBuf, String> {
        let canon_root = std::fs::canonicalize(&self.root)
            .map_err(|e| format!("cannot canonicalize root {}: {e}", self.root.display()))?;
        let rel = p.trim();
        let candidate = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            self.root.join(rel)
        };
        let norm = normalize(&candidate);
        if !norm.starts_with(&canon_root) {
            return Err(format!("path escapes working directory: {p}"));
        }
        Ok(norm)
    }
}

/// Lexical normalization (handles `..`, `.`, duplicate separators) without
/// touching the filesystem.
fn normalize(p: &Path) -> PathBuf {
    let mut out: Vec<std::path::Component<'_>> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    let mut pb = PathBuf::new();
    pb.extend(out);
    pb
}
