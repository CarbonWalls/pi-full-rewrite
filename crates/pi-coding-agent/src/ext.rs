//! pi extension protocol (v1) — subprocess JSON-lines.
//!
//! An extension is any executable that speaks this newline-delimited JSON
//! protocol on stdin/stdout (any language, no runtime dependency):
//!
//! host → ext : `{"type":"hello","version":1,"model":"...","cwd":"..."}`
//! ext → host : `{"type":"ready","tools":[{name,description,parameters}],"system":"..."}`
//! host → ext : `{"type":"call","id":N,"tool":"...","args":{...}}`
//! ext → host : `{"type":"result","id":N,"output":"...","is_error":false}`
//! host → ext : `{"type":"event","event":{...}}`   (agent-event broadcast)
//! ext → host : `{"type":"system_append","text":"..."}`
//!
//! The extension's tools are exposed as `ext:<name>_<tool>`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ExtError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad extension message: {0}")]
    BadMessage(String),
    #[error("extension closed unexpectedly")]
    Closed,
}

/// Shared handle used for event broadcasting.
pub type ExtHandle = Arc<Mutex<ExtClient>>;

/// Result of connecting one extension: (tools, system text, broadcast handle).
pub type ExtConnect = Result<(Vec<Arc<dyn Tool>>, String, ExtHandle), ExtError>;

/// One running extension process + its advertised tools.
pub struct ExtClient {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<std::process::ChildStdout>>,
    next_id: u64,
    /// System-prompt text contributed by the extension (from `ready`).
    pub system: String,
}

/// Per-extension tool adapter registered into the agent's tool registry.
pub struct ExtTool {
    pub spec: ToolSpec,
    pub server: String,
    pub client: Arc<Mutex<ExtClient>>,
}

impl ExtTool {
    fn full_name(&self) -> String {
        format!("ext:{}_{}", self.server, self.spec.name)
    }
}

impl Tool for ExtTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.full_name(),
            description: self.spec.description.clone(),
            parameters: self.spec.parameters.clone(),
        }
    }

    fn run(&self, args: serde_json::Value) -> ToolResult {
        let mut g = match self.client.lock() {
            Ok(g) => g,
            Err(_) => {
                return ToolResult {
                    output: "extension client poisoned".into(),
                    is_error: true,
                    meta: None,
                }
            }
        };
        match g.call(&self.spec.name, &args) {
            Ok((text, is_error, meta)) => ToolResult {
                output: text,
                is_error,
                meta,
            },
            Err(e) => ToolResult {
                output: format!("extension error: {e}"),
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
        format!("{} {}", self.full_name(), shown)
    }
}

impl ExtClient {
    /// Spawn and handshake. Returns the client + advertised tools.
    pub fn spawn(
        name: &str,
        command: &[String],
        model: &str,
        cwd: &str,
    ) -> Result<(Self, Vec<ToolSpec>), ExtError> {
        let argv = if command.is_empty() {
            return Err(ExtError::BadMessage(format!(
                "extension '{name}' has no command"
            )));
        } else {
            command
        };
        let mut child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or(ExtError::Closed)?;
        let stdout = child.stdout.take().ok_or(ExtError::Closed)?;
        let mut c = Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout: Some(BufReader::new(stdout)),
            next_id: 1,
            system: String::new(),
        };
        c.write_line(&json!({"type":"hello","version":1,"model":model,"cwd":cwd}))?;
        // Read until `ready`.
        let mut tools: Vec<ToolSpec> = Vec::new();
        let mut seen_ready = false;
        let reader = c.stdout.as_mut().unwrap();
        for _ in 0..100 {
            let mut line = String::new();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                continue;
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("ready") => {
                    if let Some(arr) = v.get("tools").and_then(|t| t.as_array()) {
                        for t in arr {
                            let nm = t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        tools.push(ToolSpec {
                                name: nm,
                                description: t
                                    .get("description")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .into(),
                                parameters: t
                                    .get("parameters")
                                    .or_else(|| t.get("inputSchema"))
                                    .cloned()
                                    .unwrap_or(json!({"type":"object","properties":{}})),
                            });
                        }
                    }
                    if let Some(s) = v.get("system").and_then(|s| s.as_str()) {
                        c.system = s.to_string();
                    }
                    seen_ready = true;
                    break;
                }
                Some("system_append") => {
                    if let Some(s) = v.get("text").and_then(|s| s.as_str()) {
                        c.system.push_str(s);
                    }
                }
                _ => {}
            }
        }
        if !seen_ready {
            return Err(ExtError::Closed);
        }
        Ok((c, tools))
    }

    /// Call one of the extension's tools.
    pub fn call(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<(String, bool, Option<serde_json::Value>), ExtError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_line(
            &json!({"type":"call","id":id,"tool":tool,"args":args}),
        )?;
        let reader = self.stdout.as_mut().ok_or(ExtError::Closed)?;
        for _ in 0..10_000 {
            let mut line = String::new();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                return Err(ExtError::Closed);
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                continue;
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("result") if v.get("id").and_then(|i| i.as_u64()) == Some(id) => {
                    let output = v.get("output").and_then(|o| o.as_str()).unwrap_or("").to_string();
                    let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
                    let meta = v.get("meta").cloned();
                    return Ok((output, is_error, meta.filter(|m| !m.is_null())));
                }
                Some("system_append") => {
                    if let Some(s) = v.get("text").and_then(|s| s.as_str()) {
                        self.system.push_str(s);
                    }
                }
                _ => {}
            }
        }
        Err(ExtError::Closed)
    }

    /// Broadcast an agent event to the extension (observation only).
    pub fn broadcast(&mut self, ev: &serde_json::Value) -> Result<(), ExtError> {
        self.write_line(&json!({"type":"event","event":ev}))
    }

    /// Extra system text contributed so far.
    pub fn system_text(&self) -> &str {
        &self.system
    }

    fn write_line(&mut self, msg: &serde_json::Value) -> Result<(), ExtError> {
        let mut s = msg.to_string();
        s.push('\n');
        let stdin = self.stdin.as_mut().ok_or(ExtError::Closed)?;
        stdin.write_all(s.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }

    fn kill_child(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for ExtClient {
    fn drop(&mut self) {
        self.kill_child();
    }
}

/// Spawn an extension and adapt its tools for the registry.
///
/// Returns (tools, contributed-system-text, shared client handle). The handle
/// is kept by the caller for event broadcasting.
pub fn connect(
    name: &str,
    command: &[String],
    model: &str,
    cwd: &str,
) -> ExtConnect {
    let (client, tools) = ExtClient::spawn(name, command, model, cwd)?;
    let system = client.system_text().to_string();
    let shared = Arc::new(Mutex::new(client));
    let out: Vec<Arc<dyn Tool>> = tools
        .into_iter()
        .filter(|t| !t.name.is_empty())
        .map(|t| Arc::new(ExtTool {
            spec: t,
            server: name.to_string(),
            client: shared.clone(),
        }) as Arc<dyn Tool>)
        .collect();
    Ok((out, system, shared))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT: &str = r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
send({"type":"ready","tools":[{"name":"shout","description":"SHOUTS","parameters":{"type":"object","properties":{"text":{"type":"string"}}}}],"system":"Be LOUD."})
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    m=json.loads(line)
    if m.get("type")=="call":
        send({"type":"result","id":m.get("id"),"output":"HEY: "+str(m.get("args",{}).get("text","")).upper(),"is_error":False})
"#;

    fn ext_server() -> String {
        let dir = std::env::temp_dir().join(format!("pi-ext-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ext.py");
        std::fs::write(&p, EXT).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn ext_protocol_spawn_and_call() {
        let script = ext_server();
        let (tools, system, handle) =
            super::connect("shout", &["python3".into(), script], "mock/echo", ".").unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].spec().name, "ext:shout_shout");
        assert!(system.contains("Be LOUD."));
        // drive it through the shared handle
        let (out, is_err, _meta) = handle.lock().unwrap().call("shout", &json!({"text": "yo"})).unwrap();
        assert!(!is_err);
        assert_eq!(out, "HEY: YO");
    }

    #[test]
    fn full_name_is_clean() {
        let client = ExtClient {
            child: None,
            stdin: None,
            stdout: None,
            next_id: 0,
            system: String::new(),
        };
        let shared = Arc::new(Mutex::new(client));
        let tool = ExtTool {
            spec: ToolSpec {
                name: "shout".into(),
                description: String::new(),
                parameters: json!({}),
            },
            server: "shout".into(),
            client: shared,
        };
        let n = tool.spec().name;
        eprintln!("ACTUAL={:?} bytes={:?}", n, n.as_bytes());
        assert_eq!(n, "ext:shout_shout");
    }
}
