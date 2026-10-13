//! `apply_diff`: apply a unified diff to a file (like `patch(1)` but
//! sandboxed and validated). Mirrors the reference pi's apply_patch tool.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct ApplyDiff {
    pub root: PathBuf,
}

struct Hunk {
    old_start: usize, // 1-based line in the original file
    old_count: usize,
    lines: Vec<(char, String)>, // ' ', '+', '-'
}

fn parse_diff(diff: &str) -> Result<Vec<Hunk>, String> {
    let mut hunks = Vec::new();
    let mut current: Option<Hunk> = None;
    for line in diff.lines() {
        if line.starts_with("@@ ") {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            // @@ -old_start,old_count +new_start,new_count @@
            let rest = line.trim_start_matches("@@").trim_end();
            let old_part = rest.split_whitespace().next().unwrap_or("");
            let old_spec = old_part.trim_start_matches('-');
            let (start, count) = parse_range(old_spec);
            current = Some(Hunk {
                old_start: start,
                old_count: count,
                lines: Vec::new(),
            });
        } else if let Some(h) = current.as_mut() {
            let c = line.chars().next();
            match c {
                Some(' ') => {
                    let _ = &h.lines.push((' ', line[1..].to_string()));
                }
                Some('+') => {
                    let _ = &h.lines.push(('+', line[1..].to_string()));
                }
                Some('-') => {
                    let _ = &h.lines.push(('-', line[1..].to_string()));
                }
                _ => {
                    // \\ No newline at end of file or stray line: tolerate
                    if line.starts_with('\\') {
                        continue;
                    }
                    return Err(format!("apply_diff: unparsable diff line: {line:?}"));
                }
            }
        }
        // header lines (---, +++, diff --git, index, etc.) are skipped
    }
    if let Some(h) = current.take() {
        hunks.push(h);
    }
    if hunks.is_empty() {
        return Err("apply_diff: no hunks found (empty or invalid diff)".into());
    }
    Ok(hunks)
}

fn parse_range(spec: &str) -> (usize, usize) {
    // "-12,4" -> (12,4); "-0,0" -> (0,0); "-12" -> (12,1)
    let mut it = spec.splitn(2, ',');
    let start = it.next().unwrap_or("1").parse::<usize>().unwrap_or(1);
    let count = it.next().and_then(|c| c.parse::<usize>().ok()).unwrap_or(1);
    (start.max(1), count)
}

impl ApplyDiff {
    fn resolve(&self, p: &str) -> Result<PathBuf, String> {
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

    fn apply(&self, file: &Path, hunks: &[Hunk]) -> Result<String, String> {
        let original = std::fs::read_to_string(file)
            .map_err(|e| format!("apply_diff: cannot read {}: {e}", file.display()))?;
        let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
        // lines() drops a trailing newline marker; record its presence
        let had_final_nl = original.ends_with('\n');
        let cap = lines.len();

        // Apply bottom-up so earlier line numbers stay valid.
        let mut applied = 0usize;
        for h in hunks.iter().rev() {
            let idx = h.old_start.saturating_sub(1);
            if idx > cap {
                return Err(format!(
                    "apply_diff: hunk starts at {} (file has {} lines)",
                    h.old_start,
                    cap
                ));
            }
            // verify context: the file must match the ' ' and '-' lines at idx
            let mut off = idx;
            for (tag, text) in &h.lines {
                if *tag == ' ' || *tag == '-' {
                    if off >= cap || lines[off] != *text {
                        return Err(format!(
                            "apply_diff: context mismatch at line {} (expected {text:?})",
                            off + 1
                        ));
                    }
                    off += 1;
                    if *tag == '-' {
                        applied += 1;
                    }
                }
            }
            // now rewrite: drop '-' lines, keep ' ', insert '+' lines
            let mut new_block: Vec<String> = Vec::new();
            let mut src = idx;
            for (tag, text) in &h.lines {
                match tag {
                    ' ' => {
                        new_block.push(lines[src].clone());
                        src += 1;
                    }
                    '-' => src += 1,
                    '+' => new_block.push(text.clone()),
                    _ => {}
                }
            }
            if h.old_count == 0 {
                // insert-at-point hunk (new file / pure addition)
                if idx > lines.len() {
                    return Err("apply_diff: insertion index out of range".into());
                }
                lines.splice(idx..idx, new_block);
            } else {
                let end = idx + (h.old_count);
                if end > lines.len() {
                    return Err("apply_diff: hunk extends past end of file".into());
                }
                lines.splice(idx..end, new_block);
            }
        }

        let mut out = lines.join("\n");
        if had_final_nl {
            out.push('\n');
        }
        match std::fs::write(file, out) {
            Ok(_) => Ok(format!(
                "applied {} hunk(s) to {} ({} changed)",
                hunks.len(),
                file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                applied
            )),
            Err(e) => Err(format!("apply_diff: write failed: {e}")),
        }
    }
}

impl Tool for ApplyDiff {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "apply_diff".into(),
            description: "Apply a unified diff to a file within the working directory.
{diff} is standard unified diff text (like `git diff` output: @@ -S,C +N,M @@ hunks
with ' ' context, '+' additions, '-' deletions). Applies bottom-up; exact
context match required. Returns per-hunk status or the first mismatch.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "file":{"type":"string","description":"target file path (relative to working dir)"},
                    "diff":{"type":"string","description":"unified diff text"}
                },
                "required":["file","diff"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let file = match args.get("file").and_then(|v| v.as_str()) {
            Some(f) if !f.trim().is_empty() => f.to_string(),
            _ => return ToolResult::err("apply_diff: missing 'file'"),
        };
        let diff = match args.get("diff").and_then(|v| v.as_str()) {
            Some(d) if !d.trim().is_empty() => d.to_string(),
            _ => return ToolResult::err("apply_diff: missing 'diff'"),
        };
        let path = match self.resolve(&file) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        match parse_diff(&diff).and_then(|hunks| self.apply(&path, &hunks)) {
            Ok(s) => ToolResult::ok(s),
            Err(e) => ToolResult::err(e),
        }
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = Vec::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pi-patch-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parses_and_applies_hunks() {
        let d = tmp("apply");
        std::fs::write(d.join("f.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        let diff = [
            "@@ -1,4 +1,4 @@",
            " one",
            "-two",
            "+TWO",
            " three",
            " four",
        ]
        .join("\n");
        let parser = ApplyDiff { root: d.clone() };
        let hunks = parse_diff(&diff).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        parser.apply(&d.join("f.txt"), &hunks).unwrap();
        let content = std::fs::read_to_string(d.join("f.txt")).unwrap();
        assert_eq!(content, "one\nTWO\nthree\nfour\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn insertion_hunk_works() {
        let d = tmp("insert");
        std::fs::write(d.join("g.txt"), "a\nb\n").unwrap();
        // pure-insertion hunk (zero-length old range) prepends before line 1
        let diff = ["@@ -0,0 +1,2 @@", "+mid"].join("\n");
        let parser = ApplyDiff { root: d.clone() };
        let hunks = parse_diff(&diff).unwrap();
        assert_eq!(hunks[0].old_count, 0);
        parser.apply(&d.join("g.txt"), &hunks).unwrap();
        let content = std::fs::read_to_string(d.join("g.txt")).unwrap();
        assert_eq!(content, "mid\na\nb\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn mismatch_is_rejected_without_writing() {
        let d = tmp("mismatch");
        std::fs::write(d.join("f.txt"), "aaa\nbbb\n").unwrap();
        // context line differs from the file => must be rejected
        let diff = ["@@ -1,2 +1,2 @@", " aab", "-bbb", "+ccc"].join("\n");
        let parser = ApplyDiff { root: d.clone() };
        let hunks = parse_diff(&diff).unwrap();
        let r = parser.apply(&d.join("f.txt"), &hunks);
        assert!(r.is_err(), "context mismatch must be rejected: {:?}", r);
        // file untouched
        let content = std::fs::read_to_string(d.join("f.txt")).unwrap();
        assert_eq!(content, "aaa\nbbb\n");
        let _ = std::fs::remove_dir_all(&d);
    }
}
