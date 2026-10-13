use pi_agent::session::Session;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_ai::Llm;

// Regression: large outputs (>channel capacity) must not deadlock the
// user_turn path.
#[tokio::test(flavor = "current_thread")]
async fn agent_large_prompt_completes() {
    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig::default(),
        pi_agent::tools::ToolRegistry::new(),
        Session::new(Some("big".into())),
    );
    let mut rx = agent.user_turn("word ".repeat(4000)).await;
    let mut saw_done = false;
    let mut count = 0u32;
    while let Some(ev) = rx.recv().await {
        count += 1;
        if matches!(ev, AgentEvent::AssistantMessage { .. }) { saw_done = true; }
        if count > 200_000 { break; }
    }
    assert!(saw_done, "assistant message must arrive; drained {count} events");
    assert!(count > 1000, "should stream the large output, got {count}");
}
