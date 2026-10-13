//! Binary-level MCP client E2E: `pi --mcp "srv <cmd>"` registers the remote
//! server's tools as `mcp:srv_<tool>`; a scenario drives the agent to call
//! one and report the result.

use std::process::{Command, Stdio};

const SERVER: &str = r#"import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    m = req.get("method")
    if m == "initialize":
        send({"jsonrpc":"2.0","id":req.get("id"),"result":{"protocolVersion":"2024-11-05",
              "capabilities":{"tools":{}},"serverInfo":{"name":"demo","version":"1.0"}}})
    elif m == "tools/list":
        send({"jsonrpc":"2.0","id":req.get("id"),"result":{"tools":[
            {"name":"upper","description":"uppercases text",
             "inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}})
    elif m == "tools/call":
        assert req["params"]["name"] == "upper"
        text = req["params"]["arguments"].get("text", "")
        send({"jsonrpc":"2.0","id":req.get("id"),"result":{"content":[
            {"type":"text","text": text.upper()}]}})
    elif m == "notifications/initialized":
        pass
    else:
        send({"jsonrpc":"2.0","id":req.get("id"),"error":{"code":-32601,"message":"unknown "+str(m)}})
"#;

#[test]
fn agent_calls_a_remote_mcp_tool() {
    let dir = std::env::temp_dir().join(format!("pi-mcpe2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let server = dir.join("srv.py");
    std::fs::write(&server, SERVER).unwrap();

    let scn = dir.join("scn.jsonl");
    std::fs::write(
        &scn,
        "{\"contains\":\"uppercase it\",\"tool\":\"mcp:demo_upper\",\"args\":{\"text\":\"hello mcp\"}}\n\
         {\"after_tool_contains\":\"HELLO MCP\",\"text\":\"mcp round trip ok\"}\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "--session-dir",
            dir.join(".pi").to_str().unwrap(),
            "-m",
            "mock/echo",
            "--mcp",
            &format!("demo python3 {}", server.display()),
            "please uppercase it",
        ])
        .env("PI_MOCK_SCENARIO", scn.to_str().unwrap())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(
        t.contains("mcp round trip ok"),
        "remote tool result reached the agent: {t}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
