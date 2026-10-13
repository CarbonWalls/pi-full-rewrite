//! End-to-end test of the REAL Anthropic provider path against a fake local
//! API server: request shaping (anthropic format), SSE event parsing
//! (message_start / content_block_* / message_delta / message_stop), tool-use
//! JSON deltas, real tool execution, and the tool-result round trip.

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
            let (mut sock, _) = match listener.accept() {
                Ok(t) => t,
                Err(_) => return,
            };
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
            if !reply.is_empty() {
                let _ = sock.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                );
                let _ = sock.write_all(reply.as_bytes());
                let _ = sock.flush();
            }
            drop(sock);
        }
    });
    port
}

fn event(ev: &str, data: Value) -> String {
    format!("event: {ev}\ndata: {data}\n\n")
}

fn tool_use_stream() -> String {
    let mut o = String::new();
    o.push_str(&event("message_start",
        json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":11,"cache_read_input_tokens":0,"output_tokens":1}}})));
    o.push_str(&event("content_block_start",
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"write_file"}})));
    o.push_str(&event("content_block_delta",
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"made.txt\",\"content\":\"from anthropic\"}"}})));
    o.push_str(&event("content_block_stop", json!({"type":"content_block_stop","index":0})));
    o.push_str(&event("message_delta",
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":9,"cache_creation_input_tokens":0}})));
    o.push_str(&event("message_stop", json!({"type":"message_stop"})));
    o
}

fn final_text_stream() -> String {
    let mut o = String::new();
    o.push_str(&event("message_start",
        json!({"type":"message_start","message":{"id":"msg_2","usage":{"input_tokens":40,"cache_read_input_tokens":0,"output_tokens":1}}})));
    o.push_str(&event("content_block_start",
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
    o.push_str(&event("content_block_delta",
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"anthropic final answer"}})));
    o.push_str(&event("content_block_stop", json!({"type":"content_block_stop","index":0})));
    o.push_str(&event("message_delta",
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":7}})));
    o.push_str(&event("message_stop", json!({"type":"message_stop"})));
    o
}

#[tokio::test(flavor = "current_thread")]
async fn fake_anthropic_drives_tool_loop() {
    let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let port = start_fake(recorded.clone(), vec![tool_use_stream(), final_text_stream()]);

    let dir = std::env::temp_dir().join(format!("pi-fakeant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let model = ModelInfo {
        id: "claude-fake".into(),
        provider: "anthropic".into(),
        name: "fake".into(),
        context_window: 200_000,
        max_output_tokens: 8192,
        input_price: 0.0,
        output_price: 0.0,
        reasoning: false,
        tools: true,
        vision: false,
        api: Some("anthropic-messages".into()),
        base_url: Some(format!("http://127.0.0.1:{port}")),
        sampling: None,
    };
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig { model, ..Default::default() },
        tools,
        Session::new(Some("fake-ant".into())),
    );

    let mut rx = agent.user_turn("create a file").await;
    let mut final_text = String::new();
    let mut tool_events = 0;
    while let Some(ev) = rx.recv().await {
        match ev {
            AgentEvent::ToolCallStart(c) => {
                assert_eq!(c.name, "write_file");
                tool_events += 1;
            }
            AgentEvent::AssistantMessage { message, .. }
                if message.tool_calls.is_empty() => {
                    final_text = message.text();
                }
            _ => {}
        }
    }
    std::env::remove_var("ANTHROPIC_API_KEY");
    assert_eq!(tool_events, 1, "exactly one tool call");
    assert!(final_text.contains("anthropic final answer"), "final: {final_text}");

    let content = std::fs::read_to_string(dir.join("made.txt")).unwrap();
    assert_eq!(content, "from anthropic", "file written by tool");

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2, "two API calls");
    // anthropic request shaping: tools + system array + messages
    assert!(!rec[0]["tools"].as_array().unwrap().is_empty(), "{}", rec[0]);
    // second call carries the tool_result block
    let msgs = rec[1]["messages"].as_array().unwrap().clone();
    assert!(
        msgs.iter().any(|m| m["content"].as_array().map(|a| a.iter().any(|b| b["type"] == "tool_result")).unwrap_or(false)),
        "second request must include a tool_result block: {}",
        rec[1]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
