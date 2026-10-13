//! Retry/backoff behavior: a flaky server (500, 503) must be retried and a
//! healthy response eventually returned.

use pi_ai::types::{ChatMessage, ChatRequest, ModelInfo};
use pi_ai::Llm;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// Fake server: respond `statuses[i]` on request i; the final entry is the
/// SSE body for the successful request. Records each request (method + path).
fn start_fake(responses: Vec<(u16, String)>, seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for (code, body) in responses {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let req_line = {
                let mut l = String::new();
                let _ = reader.read_line(&mut l);
                l
            };
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
            let mut b = vec![0u8; cl];
            let _ = reader.read_exact(&mut b);
            seen.lock().unwrap().push(req_line.trim().to_string());
            let head = if code == 200 {
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n".to_string()
            } else {
                format!("HTTP/1.1 {code} Too Busy\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n..")
            };
            let _ = sock.write_all(head.as_bytes());
            let _ = sock.write_all(body.as_bytes());
            let _ = sock.flush();
        }
    });
    port
}

fn sse_final(msg: &str) -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"id":"1","object":"chat.completion.chunk","choices":[
            {"index":0,"delta":{"role":"assistant","content":msg},"finish_reason":null}]})
    )
}

#[test]
fn retries_5xx_then_succeeds() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let port = start_fake(
        vec![
            (500, String::new()),
            (503, String::new()),
            (200, sse_final("recovered")),
        ],
        seen.clone(),
    );

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
    let req = ChatRequest {
        model: model.clone(),
        system: vec![],
        messages: vec![ChatMessage::text_user("hi")],
        tools: vec![],
        tool_choice: pi_ai::types::ToolChoice::Auto,
        temperature: None,
        top_p: None,
        max_tokens: None,
        thinking: pi_ai::types::ThinkingLevel::Off,
        stop: vec![],
        seed: None,
        fallback_model: None,
    };
    let mut text = String::new();
    let llm = Llm::new();
    llm.stream(&req, &mut |ev| {
        if let pi_ai::StreamEvent::TextDelta(t) = ev {
            text.push_str(&t);
        }
        true
    })
    .expect("stream should succeed after retries");

    assert_eq!(text, "recovered");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "three HTTP attempts: {seen:?}");
    assert!(seen.iter().all(|r| r.starts_with("POST")));
}

#[test]
fn non_retryable_4xx_fails_fast() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let port = start_fake(
        vec![(400, String::new())],
        seen,
    );
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
    let req = ChatRequest {
        model,
        system: vec![],
        messages: vec![ChatMessage::text_user("hi")],
        tools: vec![],
        tool_choice: pi_ai::types::ToolChoice::Auto,
        temperature: None,
        top_p: None,
        max_tokens: None,
        thinking: pi_ai::types::ThinkingLevel::Off,
        stop: vec![],
        seed: None,
        fallback_model: None,
    };
    let err = Llm::new().stream(&req, &mut |_| true).unwrap_err();
    let s = format!("{err:?}");
    assert!(s.contains("400"), "expected HTTP 400 error: {s}");
}
