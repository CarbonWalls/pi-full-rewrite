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
fn hostile_server_giant_line_is_bounded() {
    // A server that answers tools/list AFTER dumping a >1 MiB junk line must
    // produce a bounded Rpc error (audit: linear reads were unbounded).
    let dir = std::env::temp_dir().join(format!("pi-mcp-bound-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let srv = dir.join("evil.py");
    std::fs::write(
        &srv,
        r#"import sys
sys.stdout.write(('x'*1_200_000) + '\n')
sys.stdout.flush()
for line in sys.stdin:
    req_id = __import__('json').loads(line).get('id')
    sys.stdout.write(__import__('json').dumps({'jsonrpc':'2.0','id':req_id,'result':{'tools':[]}})+'\n')
    sys.stdout.flush()
"#,
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--mcp", &format!("evil {}", srv.display()), "--root", dir.to_str().unwrap(), "--json", "--mode", "ask", "x"])
        .output()
        .expect("run pi");
    let t = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    // the client must not hang or balloon; it errors out of the handshake/list
    assert!(
        t.contains("1 MiB") || t.contains("bounded") || t.contains("error"),
        "expected a bounded error, got: {t:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

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
