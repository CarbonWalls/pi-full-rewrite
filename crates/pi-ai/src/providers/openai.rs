//! OpenAI-compatible chat-completions provider (OpenAI, OpenRouter, DeepSeek,
//! Mistral, Groq, Ollama, vLLM, …).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::http::{for_each_sse, make_agent, post_with_retry};
use crate::types::{ApiError, ChatMessage, ChatRequest, ContentPart, Role, StreamEvent, ToolCall, Usage};

fn parse_usage(chunk: &Value) -> Usage {
    let u = &chunk["usage"];
    let details = u
        .get("prompt_tokens_details")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Usage {
        input: u["prompt_tokens"].as_u64().unwrap_or(0),
        output: u["completion_tokens"].as_u64().unwrap_or(0),
        // OpenAI: prompt_tokens_details.cached_tokens; DeepSeek: _prompt_cache_hit_tokens
        cache_read: details["cached_tokens"]
            .as_u64()
            .or_else(|| details["prompt_cache_hit_tokens"].as_u64())
            .unwrap_or(0),
        // cache write = newly cached prompt tokens (DeepSeek
        // prompt_cache_miss_tokens). Audit: was wrongly set from
        // completion_tokens_details.reasoning_tokens, billing reasoning as
        // input-priced cache writes.
        cache_write: details["prompt_cache_miss_tokens"].as_u64().unwrap_or(0),
        reasoning: u
            .get("completion_tokens_details")
            .and_then(|d| d["reasoning_tokens"].as_u64())
            .unwrap_or(0),
    }
}

fn to_messages(req: &ChatRequest) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    if !req.system.is_empty() {
        out.push(json!({"role":"system","content": content_value(&req.system)}));
    }
    for m in &req.messages {
        out.push(message_value(m));
    }
    out
}

fn content_value(parts: &[ContentPart]) -> Value {
    if parts.iter().all(|p| matches!(p, ContentPart::Text { .. })) {
        Value::String(parts.iter().filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join("\n"))
    } else {
        Value::Array(parts.iter().map(part_value).collect())
    }
}

fn part_value(p: &ContentPart) -> Value {
    match p {
        ContentPart::Text { text } => json!({"type":"text","text": text}),
        ContentPart::Image { media_type, source } => match source {
            crate::types::ImageSource::Base64(b64) => json!({
                "type":"image_url",
                "image_url":{"url": format!("data:{media_type};base64,{b64}")}
            }),
            crate::types::ImageSource::Url(u) => json!({
                "type":"image_url","image_url":{"url": u}
            }),
        },
        ContentPart::Reasoning { text } => json!({"type":"text","text": text}),
    }
}

fn message_value(m: &ChatMessage) -> Value {
    let mut v = json!({"role": m.role.as_str()});
    match m.role {
        Role::Tool => {
            v["tool_call_id"] = json!(m.tool_call_id.clone().unwrap_or_default());
            v["content"] = json!(m.text());
        }
        Role::Assistant if m.tool_calls.is_empty() => {
            v["content"] = json!(m.text());
        }
        _ => {
            v["content"] = content_value(&m.content);
            if !m.tool_calls.is_empty() {
                v["tool_calls"] = Value::Array(
                    m.tool_calls
                        .iter()
                        .map(|tc| {
                            json!({"id": tc.id, "type":"function","function":{"name": tc.name, "arguments": tc.arguments.to_string()}})
                        })
                        .collect(),
                );
            }
        }
    }
    v
}

/// The full streaming request with event callbacks — the entry point used by
/// the agent loop.
pub fn stream_to(
    req: &ChatRequest,
    api_key: Option<&str>,
    mut on_event: impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    let model = &req.model;
    let base = model.base_url();
    crate::http::check_base_scheme(&base)?;
    let url = format!("{base}/chat/completions");

    let mut headers: Vec<(String, String)> = vec![
        ("content-type".into(), "application/json".into()),
        ("accept".into(), "text/event-stream".into()),
    ];
    match api_key {
        Some(k) => headers.push(("authorization".into(), format!("Bearer {k}"))),
        None => {
            if model.provider == "openai" {
                return Err(ApiError::MissingKey(model.provider.clone()));
            }
        }
    }

    let mut body = json!({
        "model": model.id,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": to_messages(req),
    });
    if req.tool_choice == crate::types::ToolChoice::None {
        body["tools"] = json!([]);
        body["tool_choice"] = json!("none");
    } else if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {"name": t.name, "description": t.description, "parameters": t.parameters}
                    })
                })
                .collect(),
        );
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(p) = req.top_p {
        body["top_p"] = json!(p);
    }
    if let Some(m) = req.max_tokens {
        body["max_tokens"] = json!(m);
    }
    if !req.stop.is_empty() {
        body["stop"] = json!(req.stop);
    }
    if let Some(s) = req.seed {
        body["seed"] = json!(s);
    }
    match model.provider.as_str() {
        "deepseek" if req.thinking != crate::types::ThinkingLevel::Off => {
            body["thinking"] = json!({"type": "enabled"});
        }
        "openai" if req.thinking != crate::types::ThinkingLevel::Off && model.id.starts_with("o") => {
            body["reasoning_effort"] = json!(match req.thinking {
                crate::types::ThinkingLevel::Low => "low",
                crate::types::ThinkingLevel::Medium => "medium",
                crate::types::ThinkingLevel::High => "high",
                crate::types::ThinkingLevel::Off => "low",
            });
        }
        _ => {}
    }
    if let Some(extra) = &model.sampling {
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                body[k] = v.clone();
            }
        }
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

    let mut text: String = String::new();
    let mut reasoning: String = String::new();
    let mut calls: HashMap<u32, ToolCall> = HashMap::new();
    let mut usage = Usage::default();
    let mut stop_reason = crate::types::StopReason::Stop;
    let mut got_usage = false;

    for_each_sse(resp, &mut |ev, data| {
        if ev == "error" {
            return Err(ApiError::Stream(format!("provider error: {data}")));
        }
        // standard OpenAI stream terminator
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let chunk: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) if data.trim().is_empty() => return Ok(()),
            Err(e) => return Err(ApiError::Json(e)),
        };
        if chunk.get("usage").is_some() {
            let parsed = parse_usage(&chunk);
            if parsed != usage {
                usage = parsed;
                if !on_event(StreamEvent::Usage(usage)) {
                    return Ok(());
                }
            }
            got_usage = true;
        }
        if let Some(choices) = chunk["choices"].as_array() {
            for c in choices {
                let delta = &c["delta"];
                if let Some(t) = delta["content"].as_str() {
                    if !t.is_empty() {
                        text.push_str(t);
                        emitted = true;
                        if !on_event(StreamEvent::TextDelta(t.to_string())) {
                            return Ok(());
                        }
                    }
                }
                if let Some(r) = delta["reasoning_content"].as_str().or_else(|| {
                    delta["reasoning"]
                        .as_str()
                }) {
                    if !r.is_empty() {
                        reasoning.push_str(r);
                        if !on_event(StreamEvent::ReasoningDelta(r.to_string())) {
                            return Ok(());
                        }
                    }
                }
                if let Some(fins) = c["finish_reason"].as_str() {
                    stop_reason = match fins {
                        "tool_calls" => crate::types::StopReason::ToolCalls,
                        "length" => crate::types::StopReason::Length,
                        _ => crate::types::StopReason::Stop,
                    };
                }
                if let Some(tcs) = delta["tool_calls"].as_array() {
                    for tc in tcs {
                        let idx = tc["index"].as_u64().unwrap_or(0) as u32;
                        let entry = calls.entry(idx).or_insert_with(|| ToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: Value::Null,
                        });
                        if let Some(id) = tc["id"].as_str() {
                            entry.id = id.to_string();
                        }
                        if let Some(nm) = tc["function"]["name"].as_str() {
                            entry.name = nm.to_string();
                        }
                        if let Some(args) = tc["function"]["arguments"].as_str() {
                            if entry.arguments == Value::Null {
                                entry.arguments = Value::String(String::new());
                            }
                            if let Value::String(acc) = &mut entry.arguments {
                                acc.push_str(args);
                            }
                            if !on_event(StreamEvent::ToolDelta {
                                index: idx,
                                id: None,
                                name: None,
                                arguments_delta: args.to_string(),
                            }) {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    })?;

    // deterministic order by tool index (a sort by id is unstable for
    // providers that omit ids — audit MINOR)
    let mut items: Vec<(u32, ToolCall)> = calls.into_iter().collect();
    items.sort_by_key(|(idx, _)| *idx);
    let mut tool_calls: Vec<ToolCall> = items.into_iter().map(|(_, tc)| tc).collect();
    // rebuild arguments from JSON strings into values
    for tc in &mut tool_calls {
        if let Value::String(raw) = &tc.arguments {
            if !raw.trim().is_empty() {
                if let Ok(v) = serde_json::from_str(raw) {
                    tc.arguments = v;
                }
            } else {
                tc.arguments = json!({});
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
        tool_calls,
        tool_call_id: None,
        tool_name: None,
    };

    if !got_usage {
        on_event(StreamEvent::Usage(usage));
    }
    on_event(StreamEvent::Done {
        message,
        stop_reason,
        usage,
    });
    Ok(())
}
