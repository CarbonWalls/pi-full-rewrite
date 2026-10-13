//! Anthropic Messages API provider with prompt-cache control + extended thinking.

use serde_json::{json, Value};

use crate::http::{for_each_sse, make_agent, post_with_retry};
use crate::types::{
    ApiError, ChatMessage, ChatRequest, ContentPart, Role, StopReason, StreamEvent, ToolCall,
    Usage,
};

pub fn stream_to(
    req: &ChatRequest,
    api_key: Option<&str>,
    mut on_event: impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    let model = &req.model;
    let base = model.base_url();
    let url = format!("{base}/messages");

    let key = api_key
        .map(str::to_owned)
        .filter(|k| !k.is_empty());
    let mut headers: Vec<(String, String)> = vec![
        ("content-type".into(), "application/json".into()),
        ("anthropic-version".into(), "2023-06-01".into()),
        ("anthropic-dangerous-direct-browser-access".into(), "true".into()),
    ];
    match key {
        Some(k) => headers.push(("x-api-key".into(), k)),
        None if model.provider == "anthropic" => {
            return Err(ApiError::MissingKey(model.provider.clone()))
        }
        None => {}
    }

    // system prompt as separate field; mark last block cacheable.
    let system_blocks: Vec<Value> = req
        .system
        .iter()
        .map(|p| match p {
            ContentPart::Text { text } => json!({"type":"text","text": text}),
            _ => json!({"type":"text","text":""}),
        })
        .collect();

    let mut body = json!({
        "model": model.id,
        "stream": true,
        "max_tokens": req.max_tokens.unwrap_or(model.max_output_tokens as u32),
        "messages": anthropic_messages(req),
    });
    if !system_blocks.is_empty() {
        // cache_control on the last system block for cheap repeats
        let mut blocks = system_blocks;
        if let Some(last) = blocks.last_mut() {
            last["cache_control"] = json!({"type": "ephemeral"});
        }
        body["system"] = Value::Array(blocks);
    }
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| {
                    json!({"name": t.name, "description": t.description, "input_schema": t.parameters})
                })
                .collect(),
        );
        if req.tool_choice == crate::types::ToolChoice::None {
            body["tool_choice"] = json!({"type": "none"});
        }
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(p) = req.top_p {
        body["top_p"] = json!(p);
    }
    if !req.stop.is_empty() {
        body["stop_sequences"] = json!(req.stop);
    }
    if let Some(s) = req.seed {
        body["seed"] = json!(s);
    }
    if req.thinking != crate::types::ThinkingLevel::Off {
        let budget = match req.thinking {
            crate::types::ThinkingLevel::Low => 4096u32,
            crate::types::ThinkingLevel::Medium => 10240,
            crate::types::ThinkingLevel::High => 32768,
            crate::types::ThinkingLevel::Off => 0,
        };
        let cap = model.max_output_tokens as u32;
        let b = budget.min(cap.saturating_sub(1).max(budget));
        body["thinking"] = json!({"type":"enabled","budget_tokens": b});
    }

    let body_str = body.to_string();
    let agent = make_agent();
    let mut emitted = false;
    let resp = match post_with_retry(&agent, &url, &headers, &body_str, &mut emitted, 4) {
        Ok(resp) => resp,
        Err(e) => {
            if let Some(fb) = &req.fallback_model {
                eprintln!("[pi] provider error ({e}), falling back to {}/{}", fb.provider, fb.id);
                let mut fb_req = req.clone();
                fb_req.model = fb.clone();
                fb_req.fallback_model = None;
                return crate::providers::stream_to(&fb_req, api_key, on_event);
            }
            return Err(e);
        }
    };

    on_event(StreamEvent::Start {
        model: model.id.clone(),
    });

    let mut text = String::new();
    let mut reasoning = String::new();
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut usage = Usage::default();
    let mut stop_reason = StopReason::Stop;

    let result = for_each_sse(resp, &mut |ev, data| {
        match ev {
            "message_start" => {
                let v: Value = serde_json::from_str(data)?;
                usage.input = v["message"]["usage"]["input_tokens"]
                    .as_u64()
                    .unwrap_or(0);
                usage.cache_read = v["message"]["usage"]["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or(0);
            }
            "content_block_start" => {
                let v: Value = serde_json::from_str(data)?;
                let b = &v["content_block"];
                if b["type"].as_str() == Some("tool_use") {
                    calls.push(ToolCall {
                        id: b["id"].as_str().unwrap_or("").to_string(),
                        name: b["name"].as_str().unwrap_or("").to_string(),
                        arguments: Value::String(String::new()),
                    });
                }
            }
            "content_block_delta" => {
                let v: Value = serde_json::from_str(data)?;
                let d = &v["delta"];
                match d["type"].as_str() {
                    Some("text_delta") => {
                        let t = d["text"].as_str().unwrap_or("");
                        text.push_str(t);
                        emitted = true;
                        if !on_event(StreamEvent::TextDelta(t.to_string())) { return Err(ApiError::Aborted); }
                    }
                    Some("thinking_delta") => {
                        let t = d["thinking"].as_str().unwrap_or("");
                        reasoning.push_str(t);
                        if !on_event(StreamEvent::ReasoningDelta(t.to_string())) { return Err(ApiError::Aborted); }
                    }
                    Some("input_json_delta") => {
                        let t = d["partial_json"].as_str().unwrap_or("");
                        if let Some(c) = calls.last_mut() {
                            if let Value::String(acc) = &mut c.arguments {
                                acc.push_str(t);
                            }
                        }
                        if !on_event(StreamEvent::ToolDelta {
                            index: calls.len() as u32,
                            id: None,
                            name: None,
                            arguments_delta: t.to_string(),
                        }) {
                            return Err(ApiError::Aborted);
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                let v: Value = serde_json::from_str(data)?;
                if let Some(sr) = v["delta"]["stop_reason"].as_str() {
                    stop_reason = match sr {
                        "tool_use" => StopReason::ToolCalls,
                        "max_tokens" => StopReason::Length,
                        "end_turn" | "stop_sequence" => StopReason::Stop,
                        _ => StopReason::Stop,
                    };
                }
                if let Some(out) = v["usage"]["output_tokens"].as_u64() {
                    usage.output = out;
                }
                if let Some(cw) = v["usage"]["cache_creation_input_tokens"].as_u64() {
                    usage.cache_write = cw;
                }
            }
            "error" => {
                let v: Value = serde_json::from_str(data)?;
                return Err(ApiError::Stream(format!(
                    "anthropic error: {}",
                    v.get("error")
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| data.to_string())
                )));
            }
            _ => {}
        }
        Ok(())
    });
    result?;

    for c in calls.iter_mut() {
        if let Value::String(raw) = &c.arguments {
            if !raw.trim().is_empty() {
                if let Ok(v) = serde_json::from_str(raw) {
                    c.arguments = v;
                }
            } else {
                c.arguments = json!({});
            }
        }
    }

    let mut parts: Vec<ContentPart> = Vec::new();
    if !reasoning.is_empty() {
        parts.push(ContentPart::Reasoning { text: reasoning });
    }
    if !text.is_empty() {
        parts.push(ContentPart::Text { text });
    }
    let message = ChatMessage {
        role: Role::Assistant,
        content: parts,
        tool_calls: calls,
        tool_call_id: None,
        tool_name: None,
    };
    on_event(StreamEvent::Usage(usage));
    on_event(StreamEvent::Done {
        message,
        stop_reason,
        usage,
    });
    Ok(())
}

fn anthropic_messages(req: &ChatRequest) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::System => {
                // system is carried in `body["system"]`; fold into first user turn.
                out.push(json!({"role":"user","content": a_content(m)}));
            }
            Role::Tool => {
                out.push(json!({
                    "role":"user",
                    "content":[{"type":"tool_result","tool_use_id": m.tool_call_id.clone().unwrap_or_default(), "content": m.text()}]
                }));
            }
            Role::Assistant if m.tool_calls.is_empty() => {
                out.push(json!({"role":"assistant","content": a_content(m)}));
            }
            _ => {
                let blocks = a_blocks(m);
                out.push(json!({"role":"assistant","content": blocks}));
            }
        }
    }
    // Anthropic requires alternating user/assistant starting with user.
    let mut merged: Vec<Value> = Vec::new();
    for m in out {
        let role = m["role"].as_str().unwrap_or("user");
        if merged.is_empty() && role != "user" {
            merged.push(json!({"role":"user","content":[{"type":"text","text":""}]}));
        }
        if let Some(last) = merged.last_mut() {
            if last["role"].as_str() == Some("user") && role == "user" {
                // coalesce consecutive user turns
                let arr: Vec<Value> = m["content"]
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| {
                        let s = m["content"].as_str().unwrap_or("");
                        vec![json!({"type":"text","text": s})]
                    });
                let mut cur = last["content"]
                    .clone()
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![json!({"type":"text","text":""})]);
                cur.extend(arr);
                last["content"] = Value::Array(cur);
                continue;
            }
        }
        merged.push(m);
    }
    merged
}

fn a_content(m: &ChatMessage) -> Value {
    if m.content.iter().all(|p| matches!(p, ContentPart::Text { .. })) {
        Value::String(m.text())
    } else {
        Value::Array(a_blocks(m))
    }
}

fn a_blocks(m: &ChatMessage) -> Vec<Value> {
    let blocks: Vec<Value> = m
        .content
        .iter()
        .map(|p| match p {
            ContentPart::Text { text } => json!({"type":"text","text": text}),
            ContentPart::Image { media_type, source } => {
                let media = match source {
                    crate::types::ImageSource::Base64(b64) => {
                        json!({"type":"base64","media_type": media_type,"data": b64})
                    }
                    crate::types::ImageSource::Url(u) => json!({"type":"url","url": u}),
                };
                json!({"type":"image","source": media})
            }
            ContentPart::Reasoning { text } => json!({"type":"text","text": text}),
        })
        .collect();
    let tool_blocks: Vec<Value> = m
        .tool_calls
        .iter()
        .map(|tc| {
            json!({
                "type":"tool_use","id": tc.id,
                "name": tc.name,
                "input": match &tc.arguments {
                    Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                    v => v.clone(),
                }
            })
        })
        .collect();
    let mut all = blocks;
    all.extend(tool_blocks);
    all
}
