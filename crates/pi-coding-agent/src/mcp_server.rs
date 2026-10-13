//! `pi --mcp-server`: expose pi's whole toolset as an MCP server over stdio
//! (JSON-RPC 2.0, newline-delimited — the same framing pi's own MCP client
//! speaks). Any MCP client (IDE, Claude Desktop, agents) can then drive pi's
//! sandboxed tools directly. This is beyond the reference pi.

use pi_agent::tools::ToolRegistry;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn serve(registry: &ToolRegistry, read: impl BufRead, mut write: impl Write) -> Result<(), String> {
    for line in read.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(
                    write,
                    "{}",
                    json!({"jsonrpc":"2.0","error":{"code":-32700,"message":format!("parse error: {e}")}})
                );
                continue;
            }
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // notifications carry no id and get no response
        let is_notification = id.is_none();
        let response = handle(method, req.get("params").cloned().unwrap_or(json!({})), registry);

        if is_notification {
            continue;
        }
        let mut out = json!({"jsonrpc":"2.0","id": id.unwrap_or(Value::Null)});
        match response {
            Ok(result) => {
                out["result"] = result;
            }
            Err(e) => {
                out["error"] = json!({"code": e.0, "message": e.1});
            }
        }
        if writeln!(write, "{out}").is_err() {
            break;
        }
        if let Err(e) = write.flush() {
            let _ = e;
        }
    }
    Ok(())
}

type RpcError = (i64, String);

fn handle(method: &str, params: Value, registry: &ToolRegistry) -> Result<Value, RpcError> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "pi", "version": env!("CARGO_PKG_VERSION")}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools: Vec<Value> = registry
                .specs()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.parameters,
                    })
                })
                .collect();
            Ok(json!({"tools": tools}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).ok_or_else(|| {
                (-32602, "tools/call: missing 'name'".to_string())
            })?;
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let result = registry.execute_named(name, args);
            let text = if result.is_error {
                format!("ERROR: {}", result.output)
            } else {
                result.output
            };
            Ok(json!({
                "content": [{"type": "text", "text": text}],
                "isError": result.is_error
            }))
        }
        "resources/list" => Ok(json!({"resources": []})),
        "prompts/list" => Ok(json!({"prompts": []})),
        "notifications/initialized" => Ok(json!({})),
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ToolRegistry {
        let dir = std::env::temp_dir().join(format!("pi-mcs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let ts = pi_tools::Toolset::new(&dir);
        let mut reg = ToolRegistry::new();
        ts.register_all(&mut reg);
        reg
    }

    #[test]
    fn initialize_list_and_call() {
        let reg = registry();
        let sess = "initialize...";
        let _ = sess;
        // feed a handshake + list + a real call
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"fs\",\"arguments\":{\"op\":\"mkdir\",\"path\":\"sub\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"nope\"}\n"
        )
        .as_bytes();
        let mut out = Vec::new();
        serve(&reg, input, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "notification gets no response: {lines:?}");

        let list: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(list["id"], 2);
        assert!(list["result"]["tools"].as_array().unwrap().len() >= 9);

        let call: Value = serde_json::from_str(lines[2]).unwrap();
        assert!(!call["result"]["isError"].as_bool().unwrap());
        assert!(call["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("created"));

        let unknown: Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);

        let dir = std::env::temp_dir().join(format!("pi-mcs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(dir.join("sub"));
    }
}
