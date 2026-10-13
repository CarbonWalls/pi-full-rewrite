//! Offline mock provider: deterministic echo, tool-call simulation, and
//! scripted scenarios. Used by unit tests, integration tests, and
//! `pi --model mock/echo`.
//!
//! A scenario file (env `PI_MOCK_SCENARIO`) is JSONL rules:
//!   {"contains":"create a file","tool":"write_file","args":{"path":"a.txt","content":"hi"}}
//!   {"contains":"now read it","tool":"read_file","args":{"path":"a.txt"}}
//!   {"after_tool_contains":"wrote","text":"File created! Let me read it."}
//!   {"after_tool_contains":"content","text":"Done -- the file exists."}
//! `contains` matches the last user message; `after_tool_contains` matches
//! the last tool result. This drives full multi-step agent loops offline.

use crate::types::{
    ApiError, ChatMessage, ChatRequest, ContentPart, Role, StopReason, StreamEvent, ToolCall,
    Usage,
};

pub fn stream_to(
    req: &ChatRequest,
    mut on_event: impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    let last_user = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.text())
        .unwrap_or_default();

    let last_is_tool_result = req
        .messages
        .last()
        .map(|m| m.role == Role::Tool)
        .unwrap_or(false);
    let last_tool = req
        .messages
        .last()
        .map(|m| m.text())
        .unwrap_or_default();

    // Scenario-driven decision (deterministic multi-step behavior). A set
    // but unparseable scenario is a loud error, not a silent echo fallback.
    match scenario_rule(&last_user, &last_tool, last_is_tool_result) {
        Ok(Some(rule)) => return emit_response(req, &rule, &last_user, &mut on_event),
        Ok(None) => {}
        Err(e) => return Err(e),
    }

    // Default: after a tool result, close the loop with a final message.
    if last_is_tool_result {
        let text = format!("tool finished: {last_tool}");
        if !on_event(StreamEvent::Start {
            model: req.model.id.clone(),
        }) {
            return Ok(());
        }
        if !on_event(StreamEvent::TextDelta(text.clone())) {
            return Ok(());
        }
        if !on_event(StreamEvent::Done {
            message: ChatMessage {
                role: Role::Assistant,
                content: vec![ContentPart::Text { text }],
                tool_calls: vec![],
                tool_call_id: None,
                tool_name: None,
            },
            stop_reason: StopReason::Stop,
            usage: Usage {
                input: last_user.len() as u64,
                output: 8,
                ..Default::default()
            },
        }) {
            return Ok(());
        }
        return Ok(());
    }

    on_event(StreamEvent::Start {
        model: req.model.id.clone(),
    });

    if last_user.contains("@tool") && !req.tools.is_empty() {
        let t = &req.tools[0];
        let args = tool_placeholder_args(t);
        let call = ToolCall {
            id: "mock-1".into(),
            name: t.name.clone(),
            arguments: serde_json::Value::Object(args),
        };
        if !on_event(StreamEvent::ToolDelta {
            index: 0,
            id: Some("mock-1".into()),
            name: Some(t.name.clone()),
            arguments_delta: call.arguments.to_string(),
        }) {
            return Ok(());
        }
        let msg = ChatMessage {
            role: Role::Assistant,
            content: vec![ContentPart::Text {
                text: "calling tool".into(),
            }],
            tool_calls: vec![call],
            tool_call_id: None,
            tool_name: None,
        };
        if !on_event(StreamEvent::Done {
            message: msg,
            stop_reason: StopReason::ToolCalls,
            usage: Usage {
                input: last_user.len() as u64,
                output: 8,
                ..Default::default()
            },
        }) {
            return Ok(());
        }
        return Ok(());
    }

    let text = format!("echo: {last_user}");
    for word in text.split_inclusive(' ') {
        if !on_event(StreamEvent::TextDelta(word.to_string())) {
            break;
        }
    }
    let out_len = text.len() as u64;
    let msg = ChatMessage {
        role: Role::Assistant,
        content: vec![ContentPart::Text { text }],
        tool_calls: vec![],
        tool_call_id: None,
        tool_name: None,
    };
    if !on_event(StreamEvent::Done {
        message: msg,
        stop_reason: StopReason::Stop,
        usage: Usage {
            input: last_user.len() as u64,
            output: out_len,
            ..Default::default()
        },
    }) {
        return Ok(());
    }
    Ok(())
}

fn tool_placeholder_args(spec: &crate::types::ToolSpec) -> serde_json::Map<String, serde_json::Value> {
    spec.parameters
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| {
            props
                .keys()
                .map(|k| {
                    (
                        k.clone(),
                        serde_json::json!(format!("value for {k}")),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Emit the response chosen by a scenario rule.
fn emit_response(
    req: &ChatRequest,
    rule: &crate::scenario::Rule,
    last_user: &str,
    on_event: &mut impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    if rule.delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(rule.delay_ms));
    }
    on_event(StreamEvent::Start {
        model: req.model.id.clone(),
    });
    match &rule.response {
        crate::scenario::Response::Text(text) => {
            let t = text.clone();
            if !on_event(StreamEvent::TextDelta(t.clone())) {
                return Ok(());
            }
            let msg = ChatMessage {
                role: Role::Assistant,
                content: vec![ContentPart::Text { text: t }],
                tool_calls: vec![],
                tool_call_id: None,
                tool_name: None,
            };
            if !on_event(StreamEvent::Done {
                message: msg,
                stop_reason: StopReason::Stop,
                usage: Usage {
                    input: last_user.len() as u64,
                    output: 12,
                    ..Default::default()
                },
            }) {
                return Ok(());
            }
        }
        crate::scenario::Response::Tool { name, args } => {
            let t = req.tools.iter().find(|t| t.name == *name);
            let call = match t {
                Some(t) => ToolCall {
                    id: "mock-1".into(),
                    name: t.name.clone(),
                    arguments: args.clone(),
                },
                None => ToolCall {
                    id: "mock-1".into(),
                    name: name.clone(),
                    arguments: serde_json::json!({"error":"tool not advertised"}),
                },
            };
            if !on_event(StreamEvent::ToolDelta {
                index: 0,
                id: Some("mock-1".into()),
                name: Some(call.name.clone()),
                arguments_delta: call.arguments.to_string(),
            }) {
                return Ok(());
            }
            let msg = ChatMessage {
                role: Role::Assistant,
                content: vec![ContentPart::Text {
                    text: rule.comment.clone(),
                }],
                tool_calls: vec![call],
                tool_call_id: None,
                tool_name: None,
            };
            if !on_event(StreamEvent::Done {
                message: msg,
                stop_reason: StopReason::ToolCalls,
                usage: Usage {
                    input: last_user.len() as u64,
                    output: 12,
                    ..Default::default()
                },
            }) {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// First rule matching the current context. `Ok(None)` when the scenario
/// env var is unset or no rule matches; `Err` when the variable is set but
/// the file is missing or malformed.
fn scenario_rule(
    last_user: &str,
    last_tool: &str,
    last_is_tool: bool,
) -> Result<Option<crate::scenario::Rule>, crate::types::ApiError> {
    let path = match std::env::var("PI_MOCK_SCENARIO") {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let rules = crate::scenario::load(std::path::Path::new(&path))
        .map_err(crate::types::ApiError::Other)?;
    for r in rules {
        match r.kind {
            crate::scenario::MatchKind::OnUser => {
                // only when the last message is a user message
                if !last_is_tool && !r.pattern.is_empty() && last_user.contains(&r.pattern) {
                    return Ok(Some(r));
                }
            }
            crate::scenario::MatchKind::OnToolResult => {
                if last_is_tool && last_tool.contains(&r.pattern) {
                    return Ok(Some(r));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolSpec;

    fn full_turn(prompt: &str, scenario: &str, tools: &[String]) -> Vec<StreamEvent> {
        let req = ChatRequest {
            model: crate::types::ModelInfo {
                id: "echo".into(),
                provider: "mock".into(),
                name: "Mock Echo".into(),
                context_window: 128_000,
                max_output_tokens: 8192,
                input_price: 0.0,
                output_price: 0.0,
                reasoning: false,
                tools: true,
                vision: false,
                api: None,
                base_url: None,
                sampling: None,
            },
            system: Vec::new(),
            messages: vec![ChatMessage::text_user(prompt)],
            tools: tools
                .iter()
                .map(|n| ToolSpec {
                    name: n.clone(),
                    description: "x".into(),
                    parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
                })
                .collect(),
            tool_choice: crate::types::ToolChoice::Auto,
            temperature: None,
            top_p: None,
            max_tokens: Some(1000),
            thinking: crate::types::ThinkingLevel::Off,
            stop: Vec::new(),
            seed: None,
            fallback_model: None,
        };
        let dir = std::env::temp_dir().join(format!("pi-mock-scn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("scenario.jsonl");
        std::fs::write(&f, scenario).unwrap();
        std::env::set_var("PI_MOCK_SCENARIO", &f);
        let mut evs = Vec::new();
        // context: prepend a Tool message if needed
        let _ = scenario;
        let _ = stream_to(&req, |e| {
            evs.push(e);
            true
        });
        std::env::remove_var("PI_MOCK_SCENARIO");
        let _ = std::fs::remove_dir_all(&dir);
        evs
    }

    #[test]
    fn scenario_selects_tool_and_text() {
        let evs = full_turn(
            "please create a file",
            r#"{"contains":"create a file","tool":"write_file","args":{"path":"a.txt","content":"hi"}}"#,
            &["write_file".into()],
        );
        assert!(evs.iter().any(|e| matches!(e, StreamEvent::Done { stop_reason: StopReason::ToolCalls, .. })), "{evs:?}");
        assert!(evs.iter().any(|e| matches!(e, StreamEvent::ToolDelta { name: Some(n), .. } if n == "write_file")));

        // a `text` rule
        let evs2 = full_turn(
            "explain x",
            r#"{"contains":"explain x","text":"X is explained."}"#,
            &["write_file".into()],
        );
        assert!(evs2.iter().any(|e| matches!(e, StreamEvent::Done { stop_reason: StopReason::Stop, .. })), "{evs2:?}");
        assert!(evs2.iter().any(|e| matches!(e, StreamEvent::TextDelta(t) if t == "X is explained.")));
    }
}
