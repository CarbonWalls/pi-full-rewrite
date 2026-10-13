//! Tool abstraction for the agent loop.

use pi_ai::types::{ToolSpec};
use serde_json::Value;

/// Blocking tool execution result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
    /// Optional extra structured output shown to the UI (e.g. a diff).
    pub meta: Option<Value>,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self { output: output.into(), is_error: false, meta: None }
    }
    pub fn err(output: impl Into<String>) -> Self {
        Self { output: output.into(), is_error: true, meta: None }
    }
}

/// A tool the agent may call. Implementations are `Send` (sync + blocking).
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    /// Execute with parsed JSON arguments.
    fn run(&self, args: Value) -> ToolResult;
    /// Human-readable short summary for the UI (defaults to tool name).
    fn summary(&self, args: &Value) -> String {
        let s = args.to_string();
        format!("{} {}", self.spec().name, truncate(&s, 80))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.min(s.len())])
    }
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<std::sync::Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: std::sync::Arc<T>) {
        let a: std::sync::Arc<dyn Tool> = tool;
        self.tools.push(a);
    }

    /// Register a pre-erased tool (used by MCP / extension systems).
    pub fn register_dyn(&mut self, tool: std::sync::Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn has(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.spec().name == name)
    }

    pub fn get(&self, name: &str) -> Option<&std::sync::Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.spec().name == name)
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|t| t.spec().clone()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.spec().name.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Execute a tool call; unknown tools yield an error result.
    pub fn execute(&self, call: &pi_ai::types::ToolCall) -> ToolResult {
        match self.get(&call.name) {
            Some(t) => {
                let args = call.arguments.clone();
                let spec_name = t.spec().name.clone();
                let result = t.run(args);
                if result.is_error {
                    ToolResult {
                        output: format!("{spec_name} failed: {}", result.output),
                        ..result
                    }
                } else {
                    result
                }
            }
            None => ToolResult::err(format!(
                "unknown tool '{}': available tools: {:?}",
                call.name,
                self.names()
            )),
        }
    }

    /// Execute a named tool with raw JSON arguments (helper for tests/CLI).
    pub fn execute_named(&self, name: &str, args: serde_json::Value) -> ToolResult {
        self.execute(&pi_ai::types::ToolCall {
            id: name.to_string(),
            name: name.to_string(),
            arguments: args,
        })
    }
}
