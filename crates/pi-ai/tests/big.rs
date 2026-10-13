#[test]
fn mock_streams_large_prompt() {
    let req = pi_ai::types::ChatRequest {
        model: pi_ai::types::ModelInfo { id: "echo".into(), provider: "mock".into(), name: "m".into(), context_window: 128000, max_output_tokens: 8192, input_price: 0.0, output_price: 0.0, reasoning: false, tools: true, vision: false, api: Some("mock".into()), base_url: None, sampling: None },
        system: Vec::new(),
        messages: vec![pi_ai::types::ChatMessage::text_user(&"word ".repeat(4000))],
        tools: Vec::new(),
        tool_choice: pi_ai::types::ToolChoice::Auto,
        temperature: None, top_p: None, max_tokens: Some(100000),
        thinking: pi_ai::types::ThinkingLevel::Off,
        stop: Vec::new(), seed: None, fallback_model: None,
    };
    let mut done = false;
    let mut events = 0u32;
    let r = pi_ai::providers::mock::stream_to(&req, |e| {
        events += 1;
        if matches!(e, pi_ai::types::StreamEvent::Done { .. }) { done = true; }
        events < 100_000
    });
    assert!(r.is_ok(), "{r:?}");
    assert!(done, "Done event must fire; events={events}");
}
