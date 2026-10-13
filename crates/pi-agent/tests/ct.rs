//! current_thread runtime regression for start_turn/finish
use pi_agent::session::Session;
use pi_agent::tools::{Tool, ToolRegistry};
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_ai::types::ToolSpec;
use pi_ai::Llm;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
struct T;
impl Tool for T {
    fn spec(&self) -> ToolSpec { ToolSpec { name: "t".into(), description: "t".into(), parameters: serde_json::json!({"type":"object"}) } }
    fn run(&self, _a: Value) -> pi_agent::tools::ToolResult { pi_agent::tools::ToolResult::ok("ok") }
}

#[tokio::test(flavor = "current_thread")]
async fn current_thread_start_turn_completes() {
    let mut tools = ToolRegistry::new();
    tools.register(std::sync::Arc::new(T));
    let agent = Agent::new(Llm::new(), AgentConfig::default(), tools, Session::new(Some("ct".into())));
    let mut turn = agent.start_turn("hello @tool");
    let mut saw = 0;
    for _ in 0..1000 {
        while let Some(ev) = turn.try_recv() {
            saw += 1;
            if matches!(ev, AgentEvent::AssistantMessage { .. }) { break; }
        }
        if turn.closed() { break; }
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let back = turn.finish().await;
    assert!(saw > 0, "events produced: got 0");
    assert!(back.session.messages().len() >= 2, "messages persisted");
}
