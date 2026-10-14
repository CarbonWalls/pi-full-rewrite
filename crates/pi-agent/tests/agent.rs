//! End-to-end tests of the agent loop with the offline mock provider.

use pi_agent::session::Session;
use pi_agent::tools::{Tool, ToolRegistry};
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_ai::types::ToolSpec;
use pi_ai::Llm;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
struct EchoTool;

impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo_tool".into(),
            description: "echo back the input".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"message": {"type": "string"}},
                "required": ["message"]
            }),
        }
    }

    fn run(&self, args: Value) -> pi_agent::tools::ToolResult {
        let msg = args["message"].as_str().unwrap_or("").to_string();
        pi_agent::tools::ToolResult::ok(format!("echo: {msg}"))
    }
}

#[tokio::test]
async fn echo_turn_produces_events_and_session() {
    let llm = Llm::new();
    let mut tools = ToolRegistry::new();
    let echo = EchoTool;
    tools.register(std::sync::Arc::new(echo.clone()));

    let mut agent = Agent::new(llm, AgentConfig::default(), tools, Session::new(Some("t1".into())));
    let mut rx = agent.user_turn("hello world").await;

    let mut events = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }

    // must contain: user, assistant start, text deltas, assistant message, usage,
    let kinds: Vec<String> = events
        .iter()
        .map(|e| match e {
            AgentEvent::UserMessage(_) => "user".into(),
            AgentEvent::AssistantStart { .. } => "start".into(),
            AgentEvent::TextDelta(_) => "text".into(),
            AgentEvent::AssistantMessage { .. } => "assistant".into(),
            AgentEvent::Usage(_) => "usage".into(),
            AgentEvent::Error(e) => format!("error:{e}"),
            _ => "other".into(),
        })
        .collect();
    assert!(kinds.iter().any(|k| k == "user"), "user event missing: {kinds:?}");
    assert!(
        kinds.iter().any(|k| k == "text"),
        "text deltas missing: {kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| k == "assistant"),
        "assistant msg missing: {kinds:?}"
    );
    assert!(!kinds.iter().any(|k| k.starts_with("error:")), "unexpected error: {kinds:?}");

    // session persisted the exchange
    let msgs = agent.session.messages();
    assert!(msgs.iter().any(|m| m.role == pi_ai::types::Role::User));
    assert!(msgs.iter().any(|m| m.role == pi_ai::types::Role::Assistant));
}

#[tokio::test]
async fn tool_call_roundtrip() {
    let llm = Llm::new();
    let mut tools = ToolRegistry::new();
    let echo = EchoTool;
    tools.register(std::sync::Arc::new(echo.clone()));

    let mut agent = Agent::new(llm, AgentConfig::default(), tools, Session::new(Some("t2".into())));
    let mut rx = agent.user_turn("please @tool now").await;

    let mut saw_call = false;
    let mut saw_result = false;
    while let Some(ev) = rx.recv().await {
        match &ev {
            AgentEvent::ToolCallStart(c) => {
                assert_eq!(c.name, "echo_tool");
                saw_call = true;
            }
            AgentEvent::ToolResult { call, result } => {
                assert_eq!(call.name, "echo_tool");
                assert!(!result.is_error, "tool failed: {}", result.output);
                saw_result = true;
            }
            AgentEvent::Error(e) => panic!("error: {e}"),
            _ => {}
        }
    }
    assert!(saw_call, "no tool call seen");
    assert!(saw_result, "no tool result seen");

    // tool result persisted as a tool-role message
    let msgs = agent.session.messages();
    assert!(msgs.iter().any(|m| m.role == pi_ai::types::Role::Tool));
}

#[tokio::test]
async fn session_persists_and_reloads() {
    let dir = std::env::temp_dir().join(format!("pi-rs-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "persist-1";

    let mut s1 = Session::new(Some(id.into()));
    s1.attach_path(&dir, true);
    s1.push_message(pi_ai::types::ChatMessage::text_user("first"));
    s1.push_message(pi_ai::types::ChatMessage::text_assistant("one"));

    // reload from disk
    let mut s2 = Session::new(Some(id.into()));
    s2.attach_path(&dir, true);
    let msgs = s2.messages();
    assert_eq!(msgs.len(), 2, "expected 2 msgs, got {msgs:?}");
    assert_eq!(msgs[0].text(), "first");
    assert_eq!(msgs[1].text(), "one");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Robustness: malformed / adversarial session files must never panic and
/// must skip invalid lines.
#[test]
fn session_survives_corrupt_jsonl() {
    let dir = std::env::temp_dir().join(format!("pi-rs-fuzz-sess-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "fuzz-1";
    let p = dir.join(format!("{id}.jsonl"));
    // write one valid entry via the API (guarantees the correct shape)
    let mut seed = Session::new(Some(id.into()));
    seed.attach_path(&dir, true);
    seed.push_message(pi_ai::types::ChatMessage::text_user("valid"));
    drop(seed);

    // append garbage: random bytes, truncated JSON, cycle-inducing parents
    let mut content = String::new();
    content.push_str("{ not json at all\n");
    content.push_str("]\n");
    content.push_str("{\"type\":\"tool_result\",");
    content.push('\n');
    // truncated entry
    content.push_str(r#"{"id":"b","parent":"a","role":"assistant","text":"trun"#);
    content.push('\n');
    content.push_str("\u{0}\u{1} \u{7f}\u{8} random binary\n");
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    f.write_all(content.as_bytes()).unwrap();

    // Xorshift PRNG pseudo-fuzz: 500 random-ish lines
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut extra = String::new();
    for i in 0..500u64 {
        let r = next();
        let line = format!("{}:{}:{}", r, r % 7, i);
        extra.push_str(&line);
        extra.push('\n');
        // occasionally a structurally-valid entry with weird ids + cycles
        if i % 50 == 0 {
            extra.push_str(&format!(
                r#"{{"id":"x{i}","parent":"x{i}","type":"message","data":{{"msg":{{}},"ts_ms":0}}}}"#
            ));
            extra.push('\n');
        }
    }
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    use std::io::Write;
    f.write_all(extra.as_bytes()).unwrap();

    let mut s = Session::new(Some(id.into()));
    s.attach_path(&dir, true);
    // valid entries loaded; garbage skipped
    assert!(!s.entries.is_empty(), "valid line must load, got {}", s.entries.len());
    let msgs = s.messages();
    assert!(msgs.iter().any(|m| m.text() == "valid"));
    // branch must terminate even with cyclic parent ids
    let b = s.branch();
    assert!(b.len() <= s.entries.len());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Robustness: parent-pointer cycles must not hang the branch walk.
#[test]
fn branch_terminates_on_cycles() {
    let dir = std::env::temp_dir().join(format!("pi-rs-fuzz-branch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "fuzz-2";
    let p = dir.join(format!("{id}.jsonl"));
    // self-cycle: a.parent == a
    let content = r#"{"id":"a","parent":"a","role":"user","text":"self"}"#.to_string() + "\n";
    std::fs::write(&p, content).unwrap();
    let mut s = Session::new(Some(id.into()));
    s.attach_path(&dir, true);
    let b = s.branch();
    assert!(b.len() <= 1, "self-cycle must stop: {}", b.len());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: `start_turn`/`finish` (the TUI path) must hand the agent back
/// exactly once. Covers the finish() double-take bug found via PTY testing.
#[tokio::test]
async fn start_turn_finish_round_trips_agent() {
    let llm = Llm::new();
    let mut tools = ToolRegistry::new();
    let echo = EchoTool;
    tools.register(std::sync::Arc::new(echo));
    let agent = Agent::new(
        llm,
        AgentConfig::default(),
        tools,
        Session::new(Some("t2".into())),
    );
    let mut turn = agent.start_turn("round trip");
    let mut saw_assistant = false;
    // wait for completion then reclaim, collecting events the whole way
    while !turn.closed() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        while let Some(ev) = turn.try_recv() {
            if matches!(ev, AgentEvent::AssistantMessage { .. }) {
                saw_assistant = true;
            }
        }
    }
    let back = turn.finish().await;
    assert!(saw_assistant, "turn should have produced an assistant message");
    assert_eq!(back.session.messages().len(), 2, "user + assistant persisted");
}

/// Scenario-driven multi-step agent loop: write_file -> read_file -> done,
/// driven entirely offline by the mock's PI_MOCK_SCENARIO rules.
#[tokio::test]
async fn scenario_drives_multistep_tool_loop() {
    let dir = std::env::temp_dir().join(format!("pi-scn-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sess")).unwrap();
    let scen = dir.join("scenario.jsonl");
    std::fs::write(
        &scen,
        r#"{"contains":"create the file demo.txt","tool":"write_file","args":{"path":"demo.txt","content":"hi"},"comment":"w"}
{"after_tool_contains":"created","tool":"read_file","args":{"path":"demo.txt"},"comment":"r"}
{"after_tool_contains":"hi","text":"verified"}"#,
    )
    .unwrap();

    let mut tools = ToolRegistry::new();
    let ts = pi_tools::Toolset::new(&dir);
    ts.register_all(&mut tools);

    let mut agent = Agent::new(
        Llm::new(),
        AgentConfig::default(),
        tools,
        Session::new(Some("scn".into())),
    );
    std::env::set_var("PI_MOCK_SCENARIO", &scen);
    let mut rx = agent
        .user_turn("please create the file demo.txt and verify it")
        .await;
    let mut calls = Vec::new();
    let mut final_text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            AgentEvent::ToolCallStart(c) => calls.push(c.name),
            AgentEvent::AssistantMessage { message, .. }
                if message.tool_calls.is_empty() => {
                    final_text = message.text();
                }
            _ => {}
        }
    }
    std::env::remove_var("PI_MOCK_SCENARIO");
    assert_eq!(calls, vec!["write_file".to_string(), "read_file".to_string()], "two scripted tool calls");
    assert!(final_text.contains("verified"), "final: {final_text}");
    let content = std::fs::read_to_string(dir.join("demo.txt")).unwrap();
    assert_eq!(content.trim(), "hi");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression: after `fork_from`, the next appended entry must branch from
/// the fork point, not from the tail of the history.
#[test]
fn fork_rewinds_parent_pointer() {
    let dir = std::env::temp_dir().join(format!("pi-fork-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "fork-1";
    let mut s = Session::new(Some(id.into()));
    s.attach_path(&dir, true);
    s.push_message(pi_ai::types::ChatMessage::text_user("alpha")); // 0
    s.push_message(pi_ai::types::ChatMessage::text_assistant("A")); // 1, child of 0
    assert_eq!(s.entries.len(), 2);

    // rewind to the user message (0-based entry 0 = alpha, matching the
    // TUI's /fork contract) and branch a new reply (user pushes go through
    // push_user_message, exactly like the agent loop)
    s.fork_from(0);
    s.push_user_message(pi_ai::types::ChatMessage::text_user("beta")); // 2, child of 0
    s.push_message(pi_ai::types::ChatMessage::text_assistant("B")); // 3, chains from beta
    assert_eq!(s.entries.len(), 4);
    assert_eq!(s.entries[2].parent, s.entries[0].id, "beta must branch from user(alpha)");
    assert_eq!(s.entries[3].parent, s.entries[2].id, "reply B chains from beta");
    assert!(s.entries[1].parent == s.entries[0].id);

    // reload from disk: the branch parent survives persistence
    let mut s2 = Session::new(Some(id.into()));
    s2.attach_path(&dir, true);
    assert_eq!(s2.entries.len(), 4);
    assert_eq!(s2.entries[2].parent, s2.entries[0].id, "fork parent persists");
    let branch = s2.branch();
    // live head chain after the fork: alpha -> beta -> B; the earlier
    // assistant reply is forked away and must not be on the mainline
    assert_eq!(branch.len(), 3, "branch: {branch:?}");
    assert_eq!(branch[0].id, s2.entries[0].id);
    assert_eq!(branch[1].id, s2.entries[2].id);
    assert_eq!(branch[2].id, s2.entries[3].id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Full session durability: mixed roles, a fork, and a compaction survive a
/// write -> reload round trip with identical ids, parents and bodies.
#[test]
fn session_round_trip_keeps_structure() {
    use pi_ai::types::ChatMessage;
    let dir = std::env::temp_dir().join(format!("pi-sessrt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "rt";
    let mut s = Session::new(Some(id.into()));
    s.attach_path(&dir, true);

    s.push_user_message(ChatMessage::text_user("u1"));
    s.push_message(ChatMessage::text_assistant("a1"));
    // tool message with a tool call id
    let mut tool = ChatMessage::text_assistant("");
    tool.tool_calls = vec![pi_ai::types::ToolCall {
        id: "call-1".into(),
        name: "bash".into(),
        arguments: serde_json::json!({"cmd": "true"}),
    }];
    s.push_message(tool);
    let mut tres = ChatMessage::text_assistant("tool done");
    tres.role = pi_ai::types::Role::Tool;
    tres.tool_call_id = Some("call-1".into());
    tres.tool_name = Some("bash".into());
    s.push_message(tres);
    s.push_message(ChatMessage::text_assistant("a2"));
    // fork to entry 1 (a1) and branch (0-based fork point, matching /fork)
    s.fork_from(1);
    s.push_user_message(ChatMessage::text_user("u2 (fork)"));
    s.push_message(ChatMessage::text_assistant("a2b"));
    // compaction on the top of the branch
    s.push_compaction("summary of the fork".into());

    // reload
    let mut s2 = Session::new(Some(id.into()));
    s2.attach_path(&dir, true);
    assert_eq!(s2.entries.len(), s.entries.len());
    for (a, b) in s.entries.iter().zip(s2.entries.iter()) {
        assert_eq!(a.id, b.id, "id");
        assert_eq!(a.parent, b.parent, "parent of {}", a.id);
        assert_eq!(a.ts_ms, b.ts_ms, "ts of {}", a.id);
    }
    // body identity: find the compaction and the fork parent edge
    assert!(s2.entries.iter().any(|e| matches!(e.body, pi_agent::session::Body::Compaction { ref summary, .. } if summary == "summary of the fork")));
    let body_text = |e: &pi_agent::session::Entry| match &e.body {
        pi_agent::session::Body::Message { msg } => msg.text(),
        pi_agent::session::Body::Compaction { summary, .. } => summary.clone(),
        pi_agent::session::Body::Label { label } => label.clone(),
        pi_agent::session::Body::Usage { model, input, output, .. } => {
            format!("{model} in={input} out={output}")
        }
    };
    let fork_user = s2.entries.iter().find(|e| body_text(e).contains("u2 (fork)")).unwrap();
    assert_eq!(fork_user.parent, s2.entries[1].id, "fork branches from assistant a1");
    // the pre-fork tail is not on the head chain
    let branch: Vec<String> = s2.branch().iter().map(|e| e.id.clone()).collect();
    let old_a2 = s2.entries.iter().find(|e| body_text(e) == "a2").unwrap().id.clone();
    assert!(!branch.contains(&old_a2), "old a2 forked away: {branch:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Labels persist as Label entries and survive a reload.
#[test]
fn session_label_round_trip() {
    use pi_ai::types::ChatMessage;
    let dir = std::env::temp_dir().join(format!("pi-label-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "lbl";
    let mut s = Session::new(Some(id.into()));
    s.attach_path(&dir, true);
    s.push_user_message(ChatMessage::text_user("hi"));
    s.push_message(ChatMessage::text_assistant("yo"));
    s.push_label("my task".into());
    assert_eq!(s.label(), Some("my task"));

    let mut s2 = Session::new(Some(id.into()));
    s2.attach_path(&dir, true);
    assert_eq!(s2.label(), Some("my task"), "label survives reload");
    assert_eq!(s2.entries.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}
