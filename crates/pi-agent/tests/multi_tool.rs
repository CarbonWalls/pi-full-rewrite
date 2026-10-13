//! Parallel tool dispatch: one assistant response with TWO tool calls must
//! run both tools (spawn_blocking fan-out) and feed both results back.

use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_tools::Toolset;
use pi_ai::types::ModelInfo;
use pi_ai::Llm;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

fn start_fake(recorded: Arc<Mutex<Vec<Value>>>, replies: Vec<String>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for reply in replies {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let cl: usize = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap_or(0);
            let mut body = vec![0u8; cl];
            let _ = reader.read_exact(&mut body);
            recorded.lock().unwrap().push(serde_json::from_slice(&body).unwrap_or(Value::Null));
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.flush();
        }
    });
    port
}

fn sse(chunks: Vec<Value>) -> String {
    let mut o = String::new();
    for c in chunks {
        o.push_str(&format!(
            "data: {}\n\n",
            json!({"id": "1", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": c, "finish_reason": null}]})
        ));
    }
    o.push_str("data: [DONE]\n\n");
    o
}

fn two_tool_calls_sse() -> String {
    sse(vec![json!({
        "role": "assistant",
        "tool_calls": [
            {"index": 0, "id": "call_a", "type": "function", "function": {"name": "write_file", "arguments": "{\"path\":\"a.txt\",\"content\":\"AAAA\"}"}},
            {"index": 1, "id": "call_b", "type": "function", "function": {"name": "write_file", "arguments": "{\"path\":\"b.txt\",\"content\":\"BBBB\"}"}}
        ]
    })])
}

fn final_text_sse() -> String {
    sse(vec![json!({"role": "assistant", "content": "both files written"})])
}

#[tokio::test(flavor = "current_thread")]
async fn parallel_tool_dispatch_runs_both() {
    let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let port = start_fake(recorded.clone(), vec![two_tool_calls_sse(), final_text_sse()]);

    let dir = std::env::temp_dir().join(format!("pi-2tool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    let model = ModelInfo {
        id: "fake".into(),
        provider: "custom".into(),
        name: "fake".into(),
        context_window: 128_000,
        max_output_tokens: 8192,
        input_price: 0.0,
        output_price: 0.0,
        reasoning: false,
        tools: true,
        vision: false,
        api: Some("openai".into()),
        base_url: Some(format!("http://127.0.0.1:{port}/v1")),
        sampling: None,
    };
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig { model, ..Default::default() },
        tools,
        Session::new(Some("2tool".into())),
    );

    let mut rx = agent.user_turn("write two files").await;
    let mut started: Vec<String> = Vec::new();
    let mut results = 0;
    let mut final_text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            AgentEvent::ToolCallStart(c) => started.push(c.name.to_string()),
            AgentEvent::ToolResult { .. } => results += 1,
            AgentEvent::AssistantMessage { message, .. }
                if message.tool_calls.is_empty() => {
                    final_text = message.text();
                }
            _ => {}
        }
    }
    assert_eq!(started, vec!["write_file".to_string(), "write_file".to_string()]);
    assert_eq!(results, 2, "both tool results");
    assert!(final_text.contains("both files written"), "{final_text}");

    let a = std::fs::read_to_string(dir.join("a.txt")).unwrap();
    let b = std::fs::read_to_string(dir.join("b.txt")).unwrap();
    assert_eq!(a, "AAAA");
    assert_eq!(b, "BBBB");

    // second provider call carries BOTH tool messages
    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2);
    let tools_in_req: Vec<&str> = rec[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(tools_in_req.len(), 2, "two tool messages in the next call: {tools_in_req:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
