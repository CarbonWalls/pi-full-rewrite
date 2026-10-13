//! A persistent-in-session TODO list tool (mirrors pi's `todo`).

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Debug, Clone)]
pub struct Todo {
    pub content: String,
    pub status: TodoStatus,
}

fn status_str(s: TodoStatus) -> &'static str {
    match s {
        TodoStatus::Pending => "pending",
        TodoStatus::InProgress => "in_progress",
        TodoStatus::Done => "done",
    }
}

fn parse_status(s: &str) -> TodoStatus {
    match s {
        "in_progress" | "in-progress" | "doing" => TodoStatus::InProgress,
        "done" | "completed" => TodoStatus::Done,
        _ => TodoStatus::Pending,
    }
}

pub struct TodoTool {
    pub todos: Arc<Mutex<Vec<Todo>>>,
}

impl Tool for TodoTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "todo".into(),
            description:
                "Maintain a session TODO list. Pass the COMPLETE list every call (it is
replaced wholesale). Each item: {\"content\": string, \"status\": \"pending\" |
\"in_progress\" | \"done\"}. Mark items as you progress; keep exactly one
in_progress when working. Useful for multi-step tasks."
                    .into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "todos":{"type":"array","description":"Full TODO list (replaces previous)","items":{
                        "type":"object",
                        "properties":{
                            "content":{"type":"string"},
                            "status":{"type":"string","enum":["pending","in_progress","done"]}
                        },
                        "required":["content"]
                    }}
                },
                "required":["todos"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let arr = match args.get("todos").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return ToolResult::err("todo: `todos` must be an array"),
        };
        let mut list: Vec<Todo> = Vec::new();
        for item in arr {
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if content.is_empty() {
                continue;
            }
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .map(parse_status)
                .unwrap_or(TodoStatus::Pending);
            list.push(Todo { content, status });
        }
        let n_done = list.iter().filter(|t| t.status == TodoStatus::Done).count();
        *self.todos.lock().unwrap() = list.clone();
        let out: Vec<String> = list
            .iter()
            .map(|t| {
                let mark = match t.status {
                    TodoStatus::Done => "[x]",
                    TodoStatus::InProgress => "[~]",
                    TodoStatus::Pending => "[ ]",
                };
                format!("{:>3} {}", mark, t.content)
            })
            .collect();
        ToolResult::ok(format!(
            "TODO updated: {}/{} done\n{}",
            n_done,
            list.len(),
            out.join("\n")
        ))
    }

    fn summary(&self, args: &Value) -> String {
        let n = args.get("todos").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
        format!("todo: set {n} items")
    }
}

/// Render the current list (for the UI / tests).
pub fn list(todos: &Arc<Mutex<Vec<Todo>>>) -> String {
    let g = todos.lock().unwrap();
    if g.is_empty() {
        return "(no todos)".into();
    }
    g.iter()
        .map(|t| format!("[{}] {}", status_str(t.status), t.content))
        .collect::<Vec<_>>()
        .join("\n")
}
