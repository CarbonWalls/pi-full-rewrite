//! Google Gemini (generateContent, alt=sse streaming) provider.

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
    let key = api_key.map(str::to_owned).filter(|k| !k.is_empty());
    if key.is_none() && model.provider == "google" {
        return Err(ApiError::MissingKey(model.provider.clone()));
    }

    // Gemini models use full resource paths like models/gemini-2.5-flash
    let model_ref = if model.id.contains('/') {
        model.id.clone()
    } else {
        format!("models/{}", model.id)
    };
    let base = model.base_url();
    let url = format!("{base}/{model_ref}:streamGenerateContent?alt=sse");

    let mut headers: Vec<(String, String)> =
        vec![("content-type".into(), "application/json".into())];
    if let Some(k) = key {
        headers.push(("x-goog-api-key".into(), k));
    }

    let mut body = json!({
        "contents": gemini_contents(req),
    });
    if !req.system.is_empty() {
        body["systemInstruction"] = json!({
            "parts": req.system.iter().map(|p| match p {
                ContentPart::Text { text } => json!({"text": text}),
                ContentPart::Image { media_type, source } => {
                    let inline = match source {
                        crate::types::ImageSource::Base64(b64) => {
                            json!({"media_type": media_type, "data": b64})
                        }
                        crate::types::ImageSource::Url(u) => json!({"url": u}),
                    };
                    json!({"inlineData": inline})
                }
                ContentPart::Reasoning { .. } => json!({"text": ""}),
            }).collect::<Vec<_>>()
        });
    }
    if !req.tools.is_empty() {
        body["tools"] = json!([{"functionDeclarations": req.tools.iter().map(|t| {
            let mut d = json!({"name": t.name, "description": t.description});
            if let Value::Object(_) = t.parameters {
                d["parameters"] = t.parameters.clone();
            }
            d
        }).collect::<Vec<_>>()}]);
    }
    let mut gen_cfg = serde_json::Map::new();
    if let Some(t) = req.temperature {
        gen_cfg.insert("temperature".into(), json!(t));
    }
    if let Some(m) = req.max_tokens {
        gen_cfg.insert("maxOutputTokens".into(), json!(m));
    }
    if !req.stop.is_empty() {
        gen_cfg.insert("stopSequences".into(), json!(req.stop));
    }
    if req.thinking != crate::types::ThinkingLevel::Off {
        let budget = match req.thinking {
            crate::types::ThinkingLevel::Low => 2048i64,
            crate::types::ThinkingLevel::Medium => 8192,
            crate::types::ThinkingLevel::High => 16384,
            crate::types::ThinkingLevel::Off => 0,
        };
        gen_cfg.insert("thinkingConfig".into(), json!({"thinkingBudget": budget}));
    }
    if !gen_cfg.is_empty() {
        body["generationConfig"] = Value::Object(gen_cfg);
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
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut usage = Usage::default();
    let mut stop_reason = StopReason::Stop;

    let result = for_each_sse(resp, &mut |ev, data| {
        if ev == "error" {
            let v: Value = serde_json::from_str(data).unwrap_or(Value::Null);
            return Err(ApiError::Stream(format!(
                "gemini error: {}",
                v.get("message").map(|m| m.to_string()).unwrap_or_else(|| data.to_string())
            )));
        }
        let v: Value = serde_json::from_str(data)?;
        if let Some(cands) = v["candidates"].as_array() {
            for c in cands {
                if let Some(fr) = c["finishReason"].as_str() {
                    stop_reason = match fr {
                        "MAX_TOKENS" => StopReason::Length,
                        "STOP" => StopReason::Stop,
                        _ => stop_reason,
                    };
                }
                let parts = c["content"]["parts"].as_array().cloned().unwrap_or_default();
                for p in &parts {
                    if let Some(t) = p["text"].as_str() {
                        text.push_str(t);
                        emitted = true;
                        if !on_event(StreamEvent::TextDelta(t.to_string())) { return Err(ApiError::Aborted); }
                    }
                    if let Some(fc) = p.get("functionCall") {
                        let name = fc["name"].as_str().unwrap_or("").to_string();
                        let args = if fc["args"].is_null() {
                            json!({})
                        } else {
                            fc["args"].clone()
                        };
                        calls.push(ToolCall {
                            id: format!("fc{}", calls.len()),
                            name,
                            arguments: args,
                        });
                    }
                }
            }
        }
        if let Some(u) = v.get("usageMetadata") {
            usage.input = u["promptTokenCount"].as_u64().unwrap_or(0);
            usage.output = u["candidatesTokenCount"].as_u64().unwrap_or(0);
            usage.reasoning = u["thoughtsTokenCount"].as_u64().unwrap_or(0);
        }
        Ok(())
    });
    result?;

    let mut parts: Vec<ContentPart> = Vec::new();
    if !text.is_empty() {
        parts.push(ContentPart::Text { text });
    }
    let has_tools = !calls.is_empty();
    let message = ChatMessage {
        role: Role::Assistant,
        content: parts,
        tool_calls: calls,
        tool_call_id: None,
        tool_name: None,
    };
    if has_tools {
        stop_reason = StopReason::ToolCalls;
    }
    if !on_event(StreamEvent::Usage(usage)) {
        return Ok(());
    }
    on_event(StreamEvent::Done {
        message,
        stop_reason,
        usage,
    });
    Ok(())
}

fn gemini_contents(req: &ChatRequest) -> Value {
    let mut contents: Vec<Value> = Vec::new();
    for m in &req.messages {
        let role = match m.role {
            Role::User | Role::System => "user",
            Role::Assistant => "model",
            Role::Tool => "user",
        };
        let mut parts: Vec<Value> = match m.role {
            Role::Tool => {
                // tool results arrive as user parts with response parts
                vec![json!({"functionResponse": {
                    "name": m.tool_name.clone().unwrap_or_default(),
                    "response": {"content": m.text()}
                }})]
            }
            _ => m
                .content
                .iter()
                .map(|p| match p {
                    ContentPart::Text { text } => json!({"text": text}),
                    ContentPart::Image { media_type, source } => {
                        let inline = match source {
                            crate::types::ImageSource::Base64(b64) => {
                                json!({"mimeType": media_type, "data": b64})
                            }
                            crate::types::ImageSource::Url(u) => json!({"uri": u}),
                        };
                        json!({"inlineData": inline})
                    }
                    ContentPart::Reasoning { .. } => json!({}),
                })
                .filter(|v| !v.is_null())
                .collect::<Vec<_>>()
                .into_iter()
                .chain(
                    m.tool_calls
                        .iter()
                        .map(|tc| {
                            json!({"functionCall": {"name": tc.name, "args": tc.arguments.clone()}})
                        })
                        .collect::<Vec<_>>(),
                )
                .collect(),
        };
        if parts.is_empty() {
            parts.push(json!({"text": ""}));
        }
        contents.push(json!({"role": role, "parts": parts}));
    }
    Value::Array(contents)
}
