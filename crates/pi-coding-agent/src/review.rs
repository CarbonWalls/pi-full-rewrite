//! /review: gather git state (status + working/staged diff) into a prompt.

use std::path::Path;
use std::process::{Command, Stdio};

/// Per-section cap: large diffs get truncated, and git's *usage help* (which
/// it prints when there's no repo) is collapsed so it can't bloat the prompt.
const MAX_SECTION: usize = 20_000;
const MAX_GIT_HELP: usize = 400;

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr).to_string();
            if !e.trim().is_empty() {
                // `git` prints full usage help on errors; keep only its first
                // error line(s) so a non-repo directory doesn't flood the
                // prompt with hundreds of help lines.
                if e.starts_with("usage:") || e.contains("usage: git ") {
                    let first_line = e.lines().next().unwrap_or("(git error)").to_string();
                    if s.is_empty() {
                        s = format!("{first_line}\n[stderr] …(git help truncated)");
                    } else {
                        s.push_str(&format!("\n[stderr] {first_line} …(git help truncated)"));
                    }
                } else {
                    s.push_str(&format!("\n[stderr] {}", truncate(&e, MAX_GIT_HELP)));
                }
            }
            s
        }
        Err(e) => format!("(git failed: {e})"),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}\n…(truncated)")
    }
}

/// Build the review prompt for the working directory `root`.
pub fn review_prompt(root: &Path) -> String {
    let status = git(root, &["status", "--short", "--branch"]);
    let diff = truncate(&git(root, &["diff"]), MAX_SECTION);
    let staged = truncate(&git(root, &["diff", "--cached"]), MAX_SECTION);
    let head = truncate(&git(root, &["log", "-n", "3", "--oneline"]), MAX_SECTION);
    format!(
        "Review the changes in this working directory and report: (1) bugs or
logic errors, (2) security issues, (3) style/consistency problems, and (4) a
suggested test plan. Be specific (file:line) and prioritize by severity.

GIT STATUS:
{status}

LAST COMMITS:
{head}

UNSTAGED DIFF:
{diff}

STAGED DIFF:
{staged}"
    )
    .trim()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_repo_prompt_is_bounded() {
        // /tmp-ish temp with no git repo
        let dir = std::env::temp_dir().join(format!("pi-review-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = review_prompt(&dir);
        assert!(
            p.chars().count() < 5_000,
            "non-repo prompt must stay compact, got {} chars",
            p.chars().count()
        );
        assert!(p.starts_with("Review"), "{p}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
