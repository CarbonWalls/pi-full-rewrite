//! End-to-end test of the REAL OpenAI-compatible provider path against a
//! fake local API server (no keys/network needed). Covers: request shaping,
//! SSE parsing, tool-call deltas, executing the real `write_file` tool, the
//! tool-result round trip, and the final stop.

use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_tools::Toolset;
use pi_ai::types::ModelInfo;
use pi_ai::Llm;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Fake OpenAI-compatible server: records request bodies, replies with SSE.
fn start_fake(
    recorded: Arc<Mutex<Vec<Value>>>,
    reply0: String, // tool-call SSE for the first request
    reply1: String, // final-text SSE for the second request
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for reply in [reply0, reply1] {
            let (mut sock, _) = match listener.accept() {
                Ok(t) => t,
                Err(_) => return,
            };
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            // read request line + headers + body (content-length)
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
            recorded.lock().unwrap().push(
                serde_json::from_slice(&body).unwrap_or(Value::Null),
            );
            // respond with SSE
            let _ = sock.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            );
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.flush();
            drop(sock);
        }
    });
    port
}

fn sse(chunks: Vec<(&str, Value)>, usage: bool) -> String {
    let mut o = String::new();
    for (id, c) in chunks {
        o.push_str(&format!(
            "data: {}\n\n",
            json!({"id": id, "object": "chat.completion.chunk", "choices": [
                {"index": 0, "delta": c, "finish_reason": null}
            ]})
        ));
    }
    if usage {
        o.push_str(&format!(
            "data: {}\n\n",
            json!({"choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 9, "total_tokens": 21}})
        ));
    }
    o.push_str("data: [DONE]\n\n");
    o
}

fn tool_call_sse() -> String {
    sse(
        vec![
            (
                "1",
                json!({"role": "assistant", "content": null, "tool_calls": [
                    {"index": 0, "id": "call_1", "type": "function",
                     "function": {"name": "write_file", "arguments": ""}}] }),
            ),
            (
                "1",
                json!({"tool_calls": [
                    {"index": 0, "function": {"arguments": "{\"path\":\"made.txt\",\"content\":\"from fake api\"}"}}] }),
            ),
        ],
        true,
    )
}

fn final_text_sse() -> String {
    sse(
        vec![
            ("2", json!({"role": "assistant", "content": "final answer, file written"})),
            ("2", json!({"content": "!"})),
        ],
        true,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn fake_openai_drives_tool_loop() {
    let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let port = start_fake(recorded.clone(), tool_call_sse(), final_text_sse());

    let dir = std::env::temp_dir().join(format!("pi-fakeoa-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    let model = ModelInfo {
        id: "fake-model".into(),
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
    let llm = Llm::new();
    let mut agent = Agent::new(
        llm,
        AgentConfig {
            model,
            ..Default::default()
        },
        tools,
        Session::new(Some("fake".into())),
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
    assert_eq!(tool_events, 1, "exactly one tool call");
    assert!(final_text.contains("final answer"), "final: {final_text}");

    // the real tool executed against the fake API's arguments
    let content = std::fs::read_to_string(dir.join("made.txt")).unwrap();
    assert_eq!(content, "from fake api", "file written by tool");

    // request shaping: first call has a User message, second includes the
    // Tool result message
    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2, "two API calls: tool call then final");
    let m0 = &rec[0]["messages"][0];
    assert_eq!(m0["role"], "user");
    let roles: Vec<&str> = rec[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert!(
        roles.contains(&"tool"),
        "second request must carry the tool result: {roles:?}"
    );
    assert!(rec[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap_or("").contains("created")),
    );
    let _ = std::fs::remove_dir_all(&dir);
}
