//! Cooperative abort: setting `Agent.abort` while a turn streams through a
//! slow server must stop the turn at the next event, dropping the tail.

use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_tools::Toolset;
use pi_ai::types::ModelInfo;
use pi_ai::Llm;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn abort_stops_mid_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
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
        let first = json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"partial "},"finish_reason":null}]});
        let second = json!({"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"DROPPED"},"finish_reason":null}]});
        let _ = sock.write_all(format!("data: {first}\n\n").as_bytes());
        let _ = sock.flush();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = sock.write_all(format!("data: {second}\n\ndata: [DONE]\n\n").as_bytes());
        let _ = sock.flush();
    });

    let dir = std::env::temp_dir().join(format!("pi-abort-{}", std::process::id()));
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
        tools: false,
        vision: false,
        api: Some("openai".into()),
        base_url: Some(format!("http://127.0.0.1:{port}/v1")),
        sampling: None,
    };
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig { model, ..Default::default() },
        tools,
        Session::new(Some("abort".into())),
    );

    // abort fires at 400ms; the tail chunk only arrives at 1500ms, so the
    // abort MUST be observed at the tail's on_event and cut it.
    let abort = agent.abort.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        eprintln!("[t] abort.store at {:?}", std::time::Instant::now());
        abort.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    let _start = std::time::Instant::now();
    let mut rx = agent.user_turn("hi").await;
    let mut final_text = String::new();
    while let Some(ev) = rx.recv().await {
        if let AgentEvent::TextDelta(t) = ev {
            final_text.push_str(&t);
        }
    }
    assert!(final_text.contains("partial"), "expected the pre-abort delta: {final_text}");
    assert!(!final_text.contains("DROPPED"), "aborted tail leaked: {final_text}");
    let _ = std::fs::remove_dir_all(&dir);
}
