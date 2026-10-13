//! MCP (Model Context Protocol) client: JSON-RPC 2.0 over a stdio subprocess.
//!
//! The MCP server is an external program that speaks newline-delimited
//! JSON-RPC 2.0 on stdin/stdout. This client speaks the subset pi needs:
//! `initialize`, `tools/list`, `tools/call`, plus notifications it ignores.
//!
//! Everything is synchronous (blocking stdio) and designed to be run inside
//! the tool executor's `spawn_blocking`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::json;

/// Errors talking to an MCP server.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("io/spawn failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("json-rpc error: {0}")]
    Rpc(String),
    #[error("server returned no matching response")]
    NoResponse,
}

/// What the server reports about itself.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// A running MCP server + its tool inventory.
pub struct McpClient {
    name: String,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    next_id: u64,
}

impl McpClient {
    /// Spawn `command` (argv, no shell) and run the MCP handshake.
    pub fn spawn(name: &str, command: &[String]) -> Result<Self, McpError> {
        let argv = if command.is_empty() {
            Err(McpError::Rpc(format!("server '{name}' has no command")))?
        } else {
            command
        };
        let mut child = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or(McpError::NoResponse)?;
        let stdout = child.stdout.take().ok_or(McpError::NoResponse)?;
        let mut c = Self {
            name: name.to_string(),
            child: Some(child),
            stdin: Some(stdin),
            stdout: Some(BufReader::new(stdout)),
            next_id: 1,
        };
        // Handshake: initialize, then the initialized notification.
        c.request("initialize", json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "pi-rs", "version": env!("CARGO_PKG_VERSION")}
        }))?;
        c.notify("notifications/initialized")?;
        Ok(c)
    }

    /// The server-reported identity (best effort, may be empty if not captured).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// List the server's tools, mapped to pi tool specs.
    pub fn list_tools(&mut self) -> Result<Vec<ToolSpec>, McpError> {
        let res = self.request("tools/list", json!({}))?;
        let mut out = Vec::new();
        if let Some(tools) = res.get("tools").and_then(|t| t.as_array()) {
            for t in tools {
                let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let desc = t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let params = t.get("inputSchema").cloned().unwrap_or(json!({
                    "type": "object",
                    "properties": {}
                }));
                out.push(ToolSpec {
                    name,
                    description: desc,
                    parameters: params,
                });
            }
        }
        Ok(out)
    }

    /// Call a tool, returning its text output (is_error flag in the result).
    pub fn call_tool(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<(String, bool), McpError> {
        let res = self.request("tools/call", json!({"name": tool, "arguments": args}))?;
        let mut text = String::new();
        const CAP: usize = 1_048_576;
        let mut push = |t: &str| {
            if text.len() >= CAP {
                return;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            let room = CAP - text.len();
            text.push_str(&t[..t.len().min(room)]);
            if t.len() > room {
                text.push_str("\n…(mcp output truncated at 1 MiB)");
            }
        };
        let is_error = res.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        if let Some(content) = res.get("content").and_then(|c| c.as_array()) {
            for item in content {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            push(t);
                        }
                    }
                    Some("json") => push(&item.to_string()),
                    _ => {}
                }
            }
        }
        if let Some(structured) = res.get("structuredContent") {
            push(&structured.to_string());
        }
        Ok((text, is_error))
    }

    // ---- internals ----

    fn request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.write_line(&msg)?;

        // Read lines until we get the response with our id. Skip notifications
        // and any out-of-band messages.
        let reader = self.stdout.as_mut().ok_or(McpError::NoResponse)?;
        for _ in 0..10_000 {
            let mut line = String::new();
            // bounded line read: a hostile server must not be able to grow
            // memory without limit (same cap as the SSE layer)
            let n = {
                use std::io::Read;
                reader.by_ref().take(1_048_576).read_line(&mut line)?
            };
            if n == 0 {
                break; // EOF
            }
            if line.len() >= 1_048_576 {
                return Err(McpError::Rpc("mcp line over 1 MiB (bounded read)".into()));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue; // not JSON (e.g. server log line)
            };
            // A response has "id" and ("result" or "error").
            if v.get("method").is_some() {
                // notification or server request: acknowledge nothing, continue
                continue;
            }
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    if !err.is_null() {
                        let msg = err
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown rpc error");
                        return Err(McpError::Rpc(msg.to_string()));
                    }
                }
                return Ok(v.get("result").cloned().unwrap_or(json!(null)));
            }
        }
        Err(McpError::NoResponse)
    }

    fn notify(&mut self, method: &str) -> Result<(), McpError> {
        let msg = json!({"jsonrpc":"2.0","method":method});
        self.write_line(&msg)
    }

    fn write_line(&mut self, msg: &serde_json::Value) -> Result<(), McpError> {
        let mut s = msg.to_string();
        s.push('\n');
        let stdin = self.stdin.as_mut().ok_or(McpError::NoResponse)?;
        stdin.write_all(s.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Close stdin to let the server exit; kill as a fallback.
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// One MCP tool exposed as a pi [`Tool`].
///
/// Because tools can run in parallel, the underlying client is shared behind a
/// `Mutex` so JSON-RPC request/response stays serialized.
pub struct McpTool {
    spec: ToolSpec,
    server: String,
    client: Arc<std::sync::Mutex<McpClient>>,
}

impl McpTool {
    pub fn new(spec: ToolSpec, server: String, client: Arc<std::sync::Mutex<McpClient>>) -> Self {
        Self {
            spec,
            server,
            client,
        }
    }
}

impl Tool for McpTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: format!("mcp:{}_{}", self.server, self.spec.name),
            description: self.spec.description.clone(),
            parameters: self.spec.parameters.clone(),
        }
    }

    fn run(&self, args: serde_json::Value) -> ToolResult {
        let mut guard = match self.client.lock() {
            Ok(g) => g,
            Err(_) => {
                return ToolResult {
                    output: "mcp client poisoned".into(),
                    is_error: true,
                    meta: None,
                };
            }
        };
        match guard.call_tool(&self.spec.name, &args) {
            Ok((text, is_error)) => ToolResult {
                output: text,
                is_error,
                meta: Some(json!({"server": self.server, "tool": self.spec.name})),
            },
            Err(e) => ToolResult {
                output: format!("mcp error: {e}"),
                is_error: true,
                meta: None,
            },
        }
    }

    fn summary(&self, args: &serde_json::Value) -> String {
        let s = args.to_string();
        let shown = if s.len() > 60 {
            format!("{}…", &s[..60])
        } else {
            s
        };
        format!("mcp:{}_{} {}", self.server, self.spec.name, shown)
    }
}

/// Connect to a server and expose its tools as pi tools.
pub fn connect_tools(
    name: &str,
    command: &[String],
) -> Result<Vec<Arc<dyn Tool>>, McpError> {
    let client = McpClient::spawn(name, command)?;
    let shared = Arc::new(std::sync::Mutex::new(client));
    let specs = shared
        .lock()
        .unwrap()
        .list_tools()?;
    Ok(specs
        .into_iter()
        .map(|s| {
            Arc::new(McpTool::new(s, name.to_string(), shared.clone())) as Arc<dyn Tool>
        })
        .collect())
}
