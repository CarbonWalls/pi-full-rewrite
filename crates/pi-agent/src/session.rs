//! Session persistence: JSONL entries with parent pointers (tree of messages,
//! matching pi's session model) under `$PI_HOME/agent/sessions/`.

use std::path::{Path, PathBuf};

use pi_ai::types::{ChatMessage, ContentPart};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Message,
    Compaction,
    Label,
    Usage,
}

/// One line of a session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// parent entry id; "" for the root
    pub parent: String,
    pub kind: EntryKind,
    #[serde(flatten)]
    pub body: Body,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Body {
    Message { msg: ChatMessage },
    Compaction {
        summary: String,
        /// number of older entries dropped from this point
        dropped: usize,
        /// summary prompt used (for reference)
        prompt: Vec<ContentPart>,
    },
    Label { label: String },
    Usage {
        input: u64,
        output: u64,
        cost: f64,
        model: String,
    },
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    static CTR: OnceLock<std::sync::atomic::AtomicU64> = OnceLock::new();
    let c = CTR.get_or_init(|| AtomicU64::new(0)).fetch_add(1, Ordering::Relaxed);
    let mut h = DefaultHasher::new();
    std::process::id().hash(&mut h);
    now_ms().hash(&mut h);
    c.wrapping_mul(0x9E37_79B9_7F4A_7C15).hash(&mut h);
    h.finish().to_string()
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub path: Option<PathBuf>,
    pub entries: Vec<Entry>,
    /// current branch head (index into entries)
    pub head: usize,
}

impl Session {
    /// Start a fresh in-memory session (persisted on flush).
    pub fn new(id: Option<String>) -> Self {
        Self {
            id: id.unwrap_or_else(new_id),
            path: None,
            entries: Vec::new(),
            head: 0, // points at *next* entry index
        }
    }

    pub fn attach_path(&mut self, dir: &Path, create: bool) {
        let _ = create;
        std::fs::create_dir_all(dir).ok();
        let p = dir.join(format!("{}.jsonl", self.id));
        if p.exists() {
            self.load_file(&p);
        }
        self.path = Some(p);
    }

    fn load_file(&mut self, p: &Path) {
        let Ok(raw) = std::fs::read_to_string(p) else {
            return;
        };
        let mut head: Option<usize> = None;
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<Entry>(line) {
                let idx = self.entries.len();
                self.entries.push(e);
                head = Some(idx);
            }
        }
        self.head = head.map(|i| i + 1).unwrap_or(0);
    }

    /// Replay the current branch: root → … → head, oldest first.
    /// O(n): one index build + a pointer walk (was O(n²) via linear parent
    /// lookup — 3000-entry stats took ~0.5 s before this fix).
    pub fn branch(&self) -> Vec<&Entry> {
        let mut chain: Vec<&Entry> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut index: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::with_capacity(self.entries.len());
        for (idx, e) in self.entries.iter().enumerate() {
            index.entry(e.id.as_str()).or_insert(idx);
        }
        let mut i = self.head.checked_sub(1);
        let mut steps = 0;
        while let Some(idx) = i {
            // corrupt-file guard: never walk more entries than exist, and
            // never revisit an index (breaks parent cycles).
            if steps >= self.entries.len() || !seen.insert(idx) {
                break;
            }
            steps += 1;
            let e = &self.entries[idx];
            chain.push(e);
            if e.parent.is_empty() {
                break;
            }
            i = match index.get(e.parent.as_str()) {
                Some(&found) if found != idx => Some(found),
                _ => break,
            };
        }
        chain.reverse();
        chain
    }

    /// All messages for the LLM context on the current branch, applying the
    /// most recent compaction: everything before the last `Compaction` entry
    /// is replaced by a single user message carrying the summary.
    pub fn messages(&self) -> Vec<ChatMessage> {
        let branch = self.branch();
        let compact_idx = branch
            .iter()
            .rposition(|e| matches!(e.kind, EntryKind::Compaction));
        let mut out: Vec<ChatMessage> = Vec::new();
        if let Some(ci) = compact_idx {
            let c = &branch[ci];
            let summary = match &c.body {
                Body::Compaction { summary, .. } => summary.clone(),
                _ => String::new(),
            };
            out.push(ChatMessage::text_user(&format!(
                "[context compacted] Summary of earlier conversation: {summary}"
            )));
            for e in &branch[ci + 1..] {
                if let Body::Message { msg } = &e.body {
                    out.push(msg.clone());
                }
            }
        } else {
            for e in &branch {
                if let Body::Message { msg } = &e.body {
                    out.push(msg.clone());
                }
            }
        }
        out
    }

    /// Append a message to the current branch (and persist).
    /// Parent id for the next append = the current head tip (respects forks;
    /// without a fork this equals the last entry).
    fn current_tip(&self) -> String {
        if self.head > 0 && self.head <= self.entries.len() {
            self.entries[self.head - 1].id.clone()
        } else {
            String::new()
        }
    }

    pub fn push_message(&mut self, msg: ChatMessage) {
        let parent = self.entries.last().map(|e| e.id.clone()).unwrap_or_default();
        let entry = Entry {
            id: new_id(),
            parent,
            kind: EntryKind::Message,
            body: Body::Message { msg },
            ts_ms: now_ms(),
        };
        self.entries.push(entry);
        self.head = self.entries.len();
        self.flush();
    }

    /// Push a USER-initiated message: branches from the current head tip, so a
    /// `fork_from` rewinds where the next user turn attaches (assistant/tool
    /// replies still chain after their own user message via `push_message`).
    pub fn push_user_message(&mut self, msg: ChatMessage) {
        let parent = self.current_tip();
        let entry = Entry {
            id: new_id(),
            parent,
            kind: EntryKind::Message,
            body: Body::Message { msg },
            ts_ms: now_ms(),
        };
        self.entries.push(entry);
        self.head = self.entries.len();
        self.flush();
    }

    pub fn push_compaction(&mut self, summary: String) {
        let parent = self.entries.last().map(|e| e.id.clone()).unwrap_or_default();
        let entry = Entry {
            id: new_id(),
            parent,
            kind: EntryKind::Compaction,
            body: Body::Compaction {
                summary,
                dropped: 0,
                prompt: Vec::new(),
            },
            ts_ms: now_ms(),
        };
        self.entries.push(entry);
        self.head = self.entries.len();
        self.flush();
    }

    /// Persist a human-readable label for the current session (used by the
    /// TUI's `/label` command; surfaced in session listings).
    /// Record a per-done usage entry (query via `pi --cost` / `--stats`).
    pub fn push_usage(&mut self, input: u64, output: u64, cost: f64, model: String) {
        let parent = self
            .entries
            .last()
            .map(|e| e.id.clone())
            .unwrap_or_default();
        self.entries.push(Entry {
            id: new_id(),
            parent,
            kind: EntryKind::Usage,
            body: Body::Usage {
                input,
                output,
                cost,
                model,
            },
            ts_ms: now_ms(),
        });
        self.flush();
    }

    pub fn push_label(&mut self, label: String) {
        let parent = self.entries.last().map(|e| e.id.clone()).unwrap_or_default();
        let entry = Entry {
            id: new_id(),
            parent,
            kind: EntryKind::Label,
            body: Body::Label { label },
            ts_ms: now_ms(),
        };
        self.entries.push(entry);
        self.head = self.entries.len();
        self.flush();
    }

    /// The most recent label applied to this session (for listings).
    pub fn label(&self) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| match &e.body {
                Body::Label { label } => Some(label.as_str()),
                _ => None,
            })
    }

    /// Fork: rewind so the next push branches from entry `at`.
    ///
    /// Semantics: `head` is one past the last entry on the branch, so after
    /// `fork_from(at)` the tip is `entries[at - 1]` (0-based `at`). The next
    /// user push chains its parent from that entry; assistant/tool replies
    /// then chain from the new user message. `at == 0` starts a fresh lineage
    /// with no parent (everything before it is orphaned).
    pub fn fork_from(&mut self, at: usize) {
        self.head = at;
    }

    fn flush(&self) {
        if let Some(p) = &self.path {
            let mut f = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
            {
                Ok(f) => f,
                Err(_) => return,
            };
            if let Some(e) = self.entries.last() {
                use std::io::Write;
                if let Ok(line) = serde_json::to_string(e) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
    }

    pub fn save_full(&self) {
        if let Some(p) = &self.path {
            let mut s = String::new();
            for e in &self.entries {
                if let Ok(line) = serde_json::to_string(e) {
                    s.push_str(&line);
                    s.push('\n');
                }
            }
            // atomic replace: a kill between truncate and write can no
            // longer leave a half-empty session file (audit NIT).
            let tmp = p.with_extension("jsonl.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, p);
            }
        }
    }

    /// Stable hash of the branch (for prompt-cache keys).
    pub fn branch_hash(&self) -> String {
        let mut h = Sha256::new();
        for e in self.branch() {
            h.update(e.id.as_bytes());
        }
        hex::encode(h.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_and_garbage_lines_are_tolerated() {
        let d = std::env::temp_dir().join(format!("pi-sess-fc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("s.jsonl");
        // build a REAL-shaped line by serializing an actual Entry
        let real = Entry {
            id: "1".into(),
            parent: String::new(),
            kind: EntryKind::Label,
            body: Body::Label { label: "L".into() },
            ts_ms: 1,
        };
        let real_line = serde_json::to_string(&real).unwrap();
        std::fs::write(
            &f,
            format!(
                "{real_line}\n{{\"type\":\"future_event\",\"payload\":[\"x\"],\"id\":\"2\",\"parent\":\"1\",\"ts_ms\":2,\"extra\":{{}}}}\nnot json at all\n"
            ),
        )
        .unwrap();
        let mut s = Session::new(Some("s".into()));
        s.attach_path(&d, false);
        // garbage + unknown-type lines must not crash or corrupt the known one
        assert_eq!(s.entries.len(), 1, "known line retained");
        assert_eq!(s.entries[0].id, "1");
        let branch = s.branch();
        assert_eq!(branch.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
