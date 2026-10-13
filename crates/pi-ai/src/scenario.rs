//! Scenario files for the offline mock provider: JSONL rules that drive
//! deterministic, multi-step agent loops (see `providers/mock.rs`).

use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum MatchKind {
    /// Match a substring of the last user message.
    OnUser,
    /// Match a substring of the last tool result.
    OnToolResult,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Text(String),
    Tool { name: String, args: Value },
    /// A deliberate provider error (for offline fallback/error-path tests).
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub kind: MatchKind,
    pub pattern: String,
    pub response: Response,
    /// Short comment text used as the assistant content when calling a tool.
    pub comment: String,
    /// Optional artificial latency (ms) before the response, to simulate a
    /// slow provider in tests.
    pub delay_ms: u64,
}

/// Parse a scenario file (JSONL). A line is either
/// `{"contains":..,"tool":..,"args":..}` (user match, tool call),
/// `{"contains":..,"text":..}` (user match, plain answer),
/// `{"after_tool_contains":..,"tool":..,"args":..}` or the `text` variant.
pub fn load(path: &Path) -> Result<Vec<Rule>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("scenario: {e}"))?;
    let mut rules = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("scenario line {}: {e}", i + 1))?;
        let on_user = v.get("contains").and_then(|x| x.as_str()).unwrap_or("");
        let on_tool = v
            .get("after_tool_contains")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let kind = if !on_tool.is_empty() {
            MatchKind::OnToolResult
        } else {
            MatchKind::OnUser
        };
        let pattern = if !on_tool.is_empty() { on_tool } else { on_user }.to_string();
        let response = if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            Response::Text(t.to_string())
        } else if let Some(name) = v.get("tool").and_then(|x| x.as_str()) {
            let args = v.get("args").cloned().unwrap_or(json!({}));
            Response::Tool {
                name: name.to_string(),
                args,
            }
        } else if let Some(e) = v.get("error").and_then(|x| x.as_str()) {
            Response::Error(e.to_string())
        } else {
            return Err(format!(
                "scenario line {}: need 'text' or 'tool' or 'error'",
                i + 1
            ));
        };
        let comment = v
            .get("comment")
            .and_then(|x| x.as_str())
            .unwrap_or("calling tool")
            .to_string();
        rules.push(Rule {
            kind,
            pattern,
            response,
            comment,
            delay_ms: v.get("delay_ms").and_then(|x| x.as_u64()).unwrap_or(0),
        });
    }
    if rules.is_empty() {
        return Err("scenario: no rules".into());
    }
    Ok(rules)
}
