//! End-to-end test of the REAL Gemini provider path (alt=sse) against a fake
//! local API server: request shaping, SSE parse (candidates/parts/
//! functionCall/usageMetadata), real tool execution, and the tool-result
//! round trip.

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

fn chunk(v: Value) -> String {
    format!("data: {v}\n\n")
}

fn gemini_tool_chunk() -> String {
    chunk(json!({
        "candidates": [{
            "content": {"parts": [
                {"functionCall": {"name": "write_file", "args": {"path": "made.txt", "content": "from gemini"}}}
            ], "role": "model"},
            "finishReason": "STOP",
            "index": 0
        }],
        "usageMetadata": {"promptTokenCount": 8, "candidatesTokenCount": 10, "totalTokenCount": 18}
    }))
}

fn gemini_text_chunk() -> String {
    chunk(json!({
        "candidates": [{
            "content": {"parts": [{"text": "gemini final answer"}], "role": "model"},
            "finishReason": "STOP",
            "index": 0
        }],
        "usageMetadata": {"promptTokenCount": 30, "candidatesTokenCount": 5, "totalTokenCount": 35}
    }))
}

#[tokio::test(flavor = "current_thread")]
async fn fake_gemini_drives_tool_loop() {
    let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let port = start_fake(
        recorded.clone(),
        vec![gemini_tool_chunk(), gemini_text_chunk() + "data: {}\n\n"],
    );

    let dir = std::env::temp_dir().join(format!("pi-fakegm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    std::env::set_var("GEMINI_API_KEY", "test-key");
    let model = ModelInfo {
        id: "models/fake-model".into(),
        provider: "google".into(),
        name: "fake".into(),
        context_window: 1_000_000,
        max_output_tokens: 8192,
        input_price: 0.0,
        output_price: 0.0,
        reasoning: false,
        tools: true,
        vision: false,
        api: Some("google-generative-ai".into()),
        base_url: Some(format!("http://127.0.0.1:{port}")),
        sampling: None,
    };
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig { model, ..Default::default() },
        tools,
        Session::new(Some("fake-gm".into())),
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
    std::env::remove_var("GEMINI_API_KEY");
    assert_eq!(tool_events, 1, "exactly one tool call");
    assert!(final_text.contains("gemini final answer"), "final: {final_text}");

    let content = std::fs::read_to_string(dir.join("made.txt")).unwrap();
    assert_eq!(content, "from gemini", "file written by tool");

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2, "two API calls");
    // gemini shaping: contents[] + tools[].functionDeclarations
    assert!(!rec[0]["contents"].as_array().unwrap().is_empty(), "{}", rec[0]);
    assert!(
        !rec[0]["tools"][0]["functionDeclarations"].as_array().unwrap().is_empty(),
        "{}",
        rec[0]
    );
    // second call carries the function response part
    let s2 = rec[1].to_string();
    assert!(s2.contains("functionResponse"), "second request must include functionResponse: {s2}");
    let _ = std::fs::remove_dir_all(&dir);
}
