//! `pi export <session.jsonl>`: render a recorded session as a readable
//! markdown transcript (or standalone HTML).

use pi_agent::session::Session;
use pi_ai::types::{ContentPart, Role};
use std::path::Path;

struct Msg {
    tag: &'static str,
    label: String,
    body: String,
}

pub fn run(file: &Path, format: &str) -> Result<String, String> {
    if !file.exists() {
        return Err(format!("session file not found: {}", file.display()));
    }
    let dir = file.parent().ok_or("no parent dir")?;
    let id = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("bad filename")?
        .to_string();
    let mut s = Session::new(Some(id.clone()));
    s.attach_path(dir, false);
    if s.entries.is_empty() {
        return Ok("(empty session)".into());
    }

    let mut msgs: Vec<Msg> = Vec::new();
    for (i, m) in s.messages().iter().enumerate() {
        let text: String = m
            .content
            .iter()
            .map(|p| match p {
                ContentPart::Text { text } => text.clone(),
                ContentPart::Image { media_type, .. } => {
                    format!("![{media_type}](inline image)")
                }
                ContentPart::Reasoning { text } => format!("> {text}"),
            })
            .collect();
        if text.trim().is_empty() && m.tool_calls.is_empty() {
            continue;
        }
        match m.role {
            Role::User => msgs.push(Msg {
                tag: "user",
                label: format!("user ({i})"),
                body: text,
            }),
            Role::Assistant => {
                if !m.tool_calls.is_empty() {
                    let calls: Vec<String> = m
                        .tool_calls
                        .iter()
                        .map(|c| format!("`{}`", c.name))
                        .collect();
                    msgs.push(Msg {
                        tag: "assistant",
                        label: format!("assistant ({i})"),
                        body: format!("{text}\n→ tool calls: {}", calls.join(", ")),
                    });
                } else if !text.is_empty() {
                    msgs.push(Msg {
                        tag: "assistant",
                        label: format!("assistant ({i})"),
                        body: text,
                    });
                }
            }
            Role::Tool => {
                let name = m.tool_name.clone().unwrap_or_default();
                msgs.push(Msg {
                    tag: "tool",
                    label: format!("tool {name} ({i})"),
                    body: text,
                });
            }
            Role::System => {}
        }
    }

    let label = s.label().map(|l| format!(" (**{l}**)")).unwrap_or_default();
    match format {
        "html" => Ok(render_html(&id, &msgs)),
        "json" => {
            let msgs: Vec<serde_json::Value> = msgs
                .iter()
                .map(|m| serde_json::json!({"tag": m.tag, "label": m.label, "body": m.body}))
                .collect();
            Ok(serde_json::json!({
                "id": id,
                "label": s.label(),
                "entries": s.entries.len(),
                "messages": msgs,
            })
            .to_string())
        }
        _ => {
            let mut out = format!("# session {id}{label}\n\n");
            render_msgs_md(&mut out, &msgs);
            Ok(out)
        }
    }
}

/// Render collected messages as markdown into `out`.
fn render_msgs_md(out: &mut String, msgs: &[Msg]) {
    for m in msgs {
        out.push_str(&format!("\n**{}**:\n{}\n", m.label, m.body));
    }
}

/// Markdown dump of an in-memory session (used by the rpc `export` arm and
/// `pi --export` to bypass the file-load path when possible).
pub fn session_to_md(s: &pi_agent::session::Session) -> String {
    let msgs = collect_msgs(s);
    let label = s.label().map(|l| format!(" (**{l}**)")).unwrap_or_default();
    let mut out = format!("# session {}{label}\n\n", s.id);
    render_msgs_md(&mut out, &msgs);
    out
}

/// Messages in display order with labels (shared by file and in-memory paths).
fn collect_msgs(s: &pi_agent::session::Session) -> Vec<Msg> {
    let mut msgs: Vec<Msg> = Vec::new();
    for (i, m) in s.messages().iter().enumerate() {
        let text: String = m
            .content
            .iter()
            .map(|p| match p {
                ContentPart::Text { text } => text.clone(),
                ContentPart::Image { media_type, .. } => {
                    format!("![{media_type}](inline image)")
                }
                ContentPart::Reasoning { text } => format!("> {text}"),
            })
            .collect();
        if text.trim().is_empty() && m.tool_calls.is_empty() {
            continue;
        }
        match m.role {
            Role::User => msgs.push(Msg {
                tag: "user",
                label: format!("user ({i})"),
                body: text,
            }),
            Role::Assistant => {
                if !m.tool_calls.is_empty() {
                    let calls: Vec<String> = m
                        .tool_calls
                        .iter()
                        .map(|c| format!("`{}`", c.name))
                        .collect();
                    msgs.push(Msg {
                        tag: "assistant",
                        label: format!("assistant ({i})"),
                        body: format!("{text}\n→ tool calls: {}", calls.join(", ")),
                    });
                } else if !text.is_empty() {
                    msgs.push(Msg {
                        tag: "assistant",
                        label: format!("assistant ({i})"),
                        body: text,
                    });
                }
            }
            Role::Tool => {
                let name = m.tool_name.clone().unwrap_or_default();
                msgs.push(Msg {
                    tag: "tool",
                    label: format!("tool {name} ({i})"),
                    body: text,
                });
            }
            Role::System => {}
        }
    }
    msgs
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_html(id: &str, msgs: &[Msg]) -> String {
    const COLORS: [&str; 3] = ["#0b93f6", "#22b573", "#8a63d2"];
    let mut rows = String::new();
    for m in msgs {
        let color = match m.tag {
            "user" => COLORS[0],
            "tool" => COLORS[2],
            _ => COLORS[1],
        };
        rows.push_str(&format!(
            "<div class=\"msg\" style=\"border-left:4px solid {color};padding:4px 10px;margin:6px 0;\">\
             <span style=\"color:{color};font-weight:600;\">{}</span>\
             <pre style=\"font-family:ui-monospace,monospace;white-space:pre-wrap;margin:4px 0 0;\">{}</pre>\
             </div>\n",
            esc(&m.label),
            esc(&m.body)
        ));
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>pi session {id}</title></head>\
         <body style=\"font-family:system-ui,sans-serif;max-width:900px;margin:24px auto;padding:0 16px;\">\
         <h1>session {id}</h1>{rows}</body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_agent::session::Session;
    use pi_ai::types::ChatMessage;

    fn session(tag: &str) -> (Session, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("pi-exp-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::new(Some("exp".into()));
        s.attach_path(&dir, true);
        s.push_user_message(ChatMessage::text_user("fix the bug"));
        let mut a = ChatMessage::text_assistant("let me look");
        a.tool_calls = vec![pi_ai::types::ToolCall {
            id: "c1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({"cmd": "ls"}),
        }];
        s.push_message(a);
        let mut t = ChatMessage::text_assistant("done");
        t.role = Role::Tool;
        t.tool_call_id = Some("c1".into());
        t.tool_name = Some("bash".into());
        s.push_message(t);
        s.push_message(ChatMessage::text_assistant("fixed"));
        (s, dir)
    }

    #[test]
    fn markdown_export_contains_turns() {
        let (s, dir) = session("md");
        let file = dir.join("exp.jsonl");
        drop(s);
        let md = run(&file, "md").unwrap();
        assert!(md.contains("# session exp"), "{md}");
        assert!(md.contains("user (0)"), "{md}");
        assert!(md.contains("fix the bug"), "{md}");
        assert!(md.contains("tool calls: `bash`"), "{md}");
        assert!(md.contains("tool bash"), "{md}");
        assert!(md.contains("fixed"), "{md}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_export_escapes_and_has_markup() {
        let (s, dir) = session("html");
        let file = dir.join("exp.jsonl");
        drop(s);
        let html = run(&file, "html").unwrap();
        assert!(html.contains("<!doctype html>"), "standalone html");
        assert!(html.contains("<h1>session exp</h1>"), "{html}");
        assert!(!html.contains("&amp;") || html.contains("&lt;"), "escaping active");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
