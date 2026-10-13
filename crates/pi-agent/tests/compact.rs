//! Auto-compaction: hitting the output cap (`finish_reason: length`) with a
//! tight context window must trigger a summarize + compaction entry in the
//! session, then the turn finishes cleanly.

use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_tools::Toolset;
use pi_ai::types::ModelInfo;
use pi_ai::Llm;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

fn start_fake(replies: Vec<String>) -> u16 {
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
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.flush();
        }
    });
    port
}

/// Response with finish_reason "length" (triggers auto_compact in the loop).
fn length_stop_sse() -> String {
    format!(
        "data: {}\n\n",
        json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"short reply"},"finish_reason":"length"}]})
    )
}

/// Response for the summarize call.
fn summarize_sse() -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"id":"2","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"COMPACTED SUMMARY"},"finish_reason":"stop"}]})
    )
}

#[tokio::test(flavor = "current_thread")]
async fn length_stop_triggers_auto_compact() {
    let port = start_fake(vec![length_stop_sse(), summarize_sse()]);

    let dir = std::env::temp_dir().join(format!("pi-compact-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut tools = pi_agent::tools::ToolRegistry::new();
    Toolset::new(&dir).register_all(&mut tools);

    let model = ModelInfo {
        id: "fake".into(),
        provider: "custom".into(),
        name: "fake".into(),
        context_window: 50, // tiny: any content crosses the 0.05 threshold
        max_output_tokens: 4096,
        input_price: 0.0,
        output_price: 0.0,
        reasoning: false,
        tools: false,
        vision: false,
        api: Some("openai".into()),
        base_url: Some(format!("http://127.0.0.1:{port}/v1")),
        sampling: None,
    };
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig {
            model,
            compact_threshold: 0.01,
            ..Default::default()
        },
        tools,
        Session::new(Some("compact".into())),
    );

    let mut rx = agent.user_turn("hi").await;
    let mut compacted = false;
    while let Some(ev) = rx.recv().await {
        if matches!(ev, AgentEvent::Compacted { .. }) {
            compacted = true;
        }
    }
    assert!(compacted, "expected a Compacted event");

    // the session carries the compaction, and the user turn still completed
    let kinds: Vec<&str> = agent
        .session
        .entries
        .iter()
        .map(|e| match &e.body {
            pi_agent::session::Body::Compaction { .. } => "compact",
            _ => "msg",
        })
        .collect();
    assert!(kinds.contains(&"compact"), "session entries: {kinds:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
