//! pi-ai offline tests: catalog, mock provider, key resolution.

use pi_ai::types::{ChatMessage, ChatRequest, StreamEvent, ToolSpec, Usage};
use pi_ai::{Catalog, Keys, Llm};

#[test]
fn catalog_resolves_builtin() {
    let c = Catalog::builtin();
    let m = c.resolve("mock/echo").expect("mock/echo");
    assert_eq!(m.provider, "mock");
    let o = c.resolve("openai/gpt-4o").expect("gpt-4o");
    assert_eq!(o.resolved_api(), pi_ai::types::Api::OpenAiCompat);
    let a = c.resolve("anthropic/claude-sonnet-4-5").expect("sonnet");
    assert_eq!(a.resolved_api(), pi_ai::types::Api::Anthropic);
    let g = c.resolve("google/gemini-2.5-flash").expect("flash");
    assert_eq!(g.resolved_api(), pi_ai::types::Api::Gemini);
}

#[test]
fn catalog_overlay_adds_model() {
    let dir = std::env::temp_dir().join(format!("pi-rs-overlay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("models.json");
    std::fs::write(
        &p,
        r#"[{"id":"local-7b","provider":"vllm","name":"Local","context_window":32768,"max_output_tokens":4096,"input_price":0,"output_price":0,"tools":true,"base_url":"http://127.0.0.1:8080/v1"}]"#,
    )
    .unwrap();
    let c = Catalog::with_overlays(&[p]).unwrap();
    let m = c.resolve("vllm/local-7b").unwrap();
    assert_eq!(m.base_url(), "http://127.0.0.1:8080/v1");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mock_streams_text_and_done() {
    let llm = Llm::new();
    let req = ChatRequest {
        model: llm.model("mock/echo").unwrap(),
        system: Vec::new(),
        messages: vec![ChatMessage::text_user("hi there")],
        tools: Vec::new(),
        tool_choice: pi_ai::types::ToolChoice::Auto,
        temperature: None,
        top_p: None,
        max_tokens: None,
        thinking: pi_ai::types::ThinkingLevel::Off,
        stop: Vec::new(),
        seed: None,
        fallback_model: None,
    };
    let mut texts = Vec::new();
    let mut saw_done = false;
    let mut usage = Usage::default();
    let _ = llm.stream(&req, |ev| match ev {
        StreamEvent::TextDelta(t) => {
            texts.push(t);
            true
        }
        StreamEvent::Usage(u) => {
            usage = u;
            true
        }
        StreamEvent::Done { usage: u, .. } => {
            saw_done = true;
            usage = u;
            true
        }
        _ => true,
    });
    assert!(saw_done, "no Done event");
    let joined: String = texts.join("");
    assert!(joined.contains("hi there"), "echo missing: {joined}");
    assert!(usage.input > 0);
}

#[test]
fn mock_tool_call_simulation() {
    let llm = Llm::new();
    let tool = ToolSpec {
        name: "add".into(),
        description: "add numbers".into(),
        parameters: serde_json::json!({"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]}),
    };
    let req = ChatRequest {
        model: llm.model("mock/echo").unwrap(),
        system: Vec::new(),
        messages: vec![ChatMessage::text_user("use @tool")],
        tools: vec![tool],
        tool_choice: pi_ai::types::ToolChoice::Auto,
        temperature: None,
        top_p: None,
        max_tokens: None,
        thinking: pi_ai::types::ThinkingLevel::Off,
        stop: Vec::new(),
        seed: None,
        fallback_model: None,
    };
    let mut done_msg: Option<ChatMessage> = None;
    let _ = llm.stream(&req, |ev| match ev {
        StreamEvent::Done { message, .. } => {
            done_msg = Some(message);
            true
        }
        _ => true,
    });
    let m = done_msg.expect("no Done");
    assert_eq!(m.tool_calls.len(), 1);
    assert_eq!(m.tool_calls[0].name, "add");
    assert_eq!(m.tool_calls[0].arguments["a"], "value for a");
}

#[test]
fn keys_resolve_by_provider() {
    let mut k = Keys::default();
    k.provider.insert("anthropic".into(), "sk-ant-x".into());
    let m = Catalog::builtin()
        .resolve("anthropic/claude-sonnet-4-5")
        .unwrap();
    assert_eq!(k.for_model(&m).as_deref(), Some("sk-ant-x"));
    let local = Catalog::builtin().resolve("ollama/llama-3.3-70b-instruct").unwrap();
    assert!(k.for_model(&local).is_none(), "no key needed for local");
}
