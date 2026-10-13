//! Fallback model: when the primary provider's connection fails (after
//! retries), the agent automatically routes to `fallback_model`.

use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_tools::Toolset;
use pi_ai::types::ModelInfo;
use pi_ai::Llm;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Fake server. When `always_500`, every request gets 500 + Retry-After: 0.
/// Otherwise replies with one SSE text chunk.
fn start_fake(requests: Arc<Mutex<u32>>, always_500: bool, text: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let text = text.to_string();
    std::thread::spawn(move || loop {
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
        *requests.lock().unwrap() += 1;
        if always_500 {
            let _ = sock.write_all(b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n..");
        } else {
            let chunk = json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":text},"finish_reason":null}]});
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(body.as_bytes());
        }
        let _ = sock.flush();
    });
    port
}

fn model(base: String, id: &str) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        provider: "custom".into(),
        name: id.into(),
        context_window: 128_000,
        max_output_tokens: 8192,
        input_price: 0.0,
        output_price: 0.0,
        reasoning: false,
        tools: false,
        vision: false,
        api: Some("openai".into()),
        base_url: Some(base),
        sampling: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn fallback_used_when_primary_errors() {
    let primary_hits = Arc::new(Mutex::new(0u32));
    let backup_hits = Arc::new(Mutex::new(0u32));
    let pport = start_fake(primary_hits.clone(), true, "");
    let bport = start_fake(backup_hits.clone(), false, "from the fallback");

    let dir = std::env::temp_dir().join(format!("pi-fallback-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    let primary = model(format!("http://127.0.0.1:{pport}/v1"), "primary");
    let backup = model(format!("http://127.0.0.1:{bport}/v1"), "backup");
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig {
            model: primary,
            fallback_model: Some(backup),
            ..Default::default()
        },
        tools,
        Session::new(Some("fb".into())),
    );

    let mut rx = agent.user_turn("hi").await;
    let mut final_text = String::new();
    while let Some(ev) = rx.recv().await {
        if let AgentEvent::AssistantMessage { message, .. } = ev {
            if message.tool_calls.is_empty() {
                final_text = message.text();
            }
        }
    }
    assert_eq!(final_text, "from the fallback");
    assert!(*primary_hits.lock().unwrap() >= 1, "primary was attempted");
    assert_eq!(*backup_hits.lock().unwrap(), 1, "fallback served exactly once");
    let _ = std::fs::remove_dir_all(&dir);
}
