//! Integration test: spawn a tiny Python MCP server and drive it through the
//! client + Tool trait.

use pi_agent::tools::ToolRegistry;
use pi_ai::types::ToolCall;
use pi_mcp::McpClient;

const SERVER: &str = r#"
import json, sys, threading

def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()

def tools():
    return {
        "result": {
            "tools": [
                {"name": "echo", "description": "echoes its input",
                 "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}},
                {"name": "fail", "description": "always errors",
                 "inputSchema": {"type": "object", "properties": {}}},
            ]
        }
    }

def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        m = json.loads(line)
        mid = m.get("id")
        method = m.get("method")
        if method == "initialize":
            send({"jsonrpc": "2.0", "id": mid, "result": {
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "py-mock", "version": "1"},
                "capabilities": {"tools": {}}}})
        elif method == "tools/list":
            send({"jsonrpc": "2.0", "id": mid, **tools()})
        elif method == "tools/call":
            args = m["params"]["arguments"]
            name = m["params"]["name"]
            if name == "echo":
                send({"jsonrpc": "2.0", "id": mid, "result": {
                    "content": [{"type": "text", "text": "echo: " + str(args.get("text", ""))}]}})
            elif name == "fail":
                send({"jsonrpc": "2.0", "id": mid, "result": {
                    "content": [{"type": "text", "text": "boom"}], "isError": True}})
            else:
                send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "unknown tool"}})
        else:
            pass  # notification
    # if stdin closes, exit
    sys.exit(0)

main()
"#;

fn spawn_server() -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("pi-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("server.py");
    std::fs::write(&p, SERVER).unwrap();
    vec!["python3".into(), p.to_string_lossy().into()]
}

#[test]
fn mcp_initialize_list_call() {
    let cmd = spawn_server();
    let mut c = McpClient::spawn("mock", &cmd).expect("spawn");
    let tools = c.list_tools().expect("list");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"echo"));
    assert!(names.contains(&"fail"));
    assert_eq!(tools[0].parameters["type"], "object");

    let (out, err) = c.call_tool("echo", &serde_json::json!({"text": "hi"})).unwrap();
    assert_eq!(out, "echo: hi");
    assert!(!err);

    let (out, err) = c.call_tool("fail", &serde_json::json!({})).unwrap();
    assert_eq!(out, "boom");
    assert!(err);

    // unknown tool -> rpc error
    let res = c.call_tool("nope", &serde_json::json!({}));
    assert!(res.is_err());
}

#[test]
fn mcp_tool_in_registry() {
    let cmd = spawn_server();
    let client = McpClient::spawn("mock", &cmd).expect("spawn");
    use pi_mcp::{McpTool};
    use std::sync::{Arc, Mutex};
    let shared = Arc::new(Mutex::new(client));
    let specs = shared.lock().unwrap().list_tools().unwrap();
    let mut reg = ToolRegistry::new();
    let echo = specs.iter().find(|s| s.name == "echo").unwrap().clone();
    let tool = Arc::new(McpTool::new(echo, "mock".into(), shared));
    reg.register(tool);
    assert!(reg.has("mcp:mock_echo"));
    let r = reg.execute(&ToolCall {
        id: "1".into(),
        name: "mcp:mock_echo".into(),
        arguments: serde_json::json!({"text": "registry"}),
    });
    assert!(!r.is_error);
    assert_eq!(r.output, "echo: registry");
}
