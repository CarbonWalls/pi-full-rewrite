//! `pi --rpc`: newline-delimited JSON protocol over stdio.
//!
//! Requests (one JSON object per line on stdin):
//! ```json
//! {"id":1,"type":"prompt","input":"..."}   -> {"id":1,"type":"event","event":{...}}*
//!                                            -> {"id":1,"type":"done","usage":{...}}
//! {"type":"tools"}                          -> {"type":"tools_result","tools":[...]}
//! {"id":2,"type":"model","ref":"a/b"}       -> {"id":2,"type":"model_result","ok":true}
//! {"type":"compact"}                        -> {"type":"compact_result","ok":true}
//! {"type":"quit"}
//!
//! Stdio RPC is a single connection: a running prompt blocks the frame
//! reader, so there is no mid-turn abort frame here (interrupt the process
//! itself, e.g. Ctrl-C). The HTTP /rpc endpoint DOES support mid-turn
//! {"type":"abort"} over a separate connection (browser stop button).
//! ```

use pi_agent::{Agent, AgentEvent};
use pi_tools::Toolset;
use serde_json::json;
use std::io::Write;

pub async fn serve(mut agent: Agent, toolset: Option<Toolset>, cli: &crate::Cli) -> anyhow::Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout());
    writeln!(out, "{}", json!({"type":"ready","model":format!("{}/{}", agent.config.model.provider, agent.config.model.id),"tools":agent.tools.names()}))?;
    out.flush()?;

    for line in std::io::stdin().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                writeln!(out, "{}", json!({"type":"error","message":format!("bad json: {e}")}))?;
                out.flush()?;
                continue;
            }
        };
        let rtype = req.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match rtype {
            "prompt" => {
                let id = req.get("id").cloned().unwrap_or(json!(null));
                let input = req.get("input").and_then(|v| v.as_str()).unwrap_or("");
                let input = input.to_string();
                // explicit `parts` win; otherwise build from input with the
                // same in-line @image attachment as the CLI (CLI parity)
                let parts = match req.get("parts") {
                    Some(p) => serde_json::from_value(p.clone()).unwrap_or_else(|_| {
                        vec![pi_ai::types::ContentPart::Text { text: input.clone() }]
                    }),
                    None => crate::inline_image_parts(&input, &cli.root),
                };
                run_prompt_parts(&mut agent, id, parts, &mut out).await;
            }
            "tools" => {
                writeln!(out, "{}", json!({"type":"tools_result","tools":agent.tools.names()}))?;
            }
            "model" => {
                let refstr = req.get("ref").and_then(|v| v.as_str()).unwrap_or("");
                let ok = if refstr.is_empty() {
                    true
                } else {
                    match agent.llm.model(refstr) {
                        Ok(m) => {
                            agent.config.model = m;
                            true
                        }
                        Err(e) => {
                            writeln!(out, "{}", json!({"type":"error","message":format!("model: {e}")}))?;
                            false
                        }
                    }
                };
                writeln!(
                    out,
                    "{}",
                    json!({"type":"model_result","ok":ok,"model":agent.config.model.id})
                )?;
            }
            "compact" => {
                agent.compact_now().await;
                writeln!(out, "{}", json!({"type":"compact_result","ok":true}))?;
            }
            "usage" => {
                writeln!(
                    out,
                    "{}",
                    json!({"type":"usage","input":agent.total_usage.input,"output":agent.total_usage.output,"cost":agent.total_cost})
                )?;
            }
            "session" => {
                writeln!(out, "{}", json!({"type":"session_result","id":agent.session.id}))?;
            }
            "export" => {
                // dump the session as markdown (mirrors `pi --export`)
                let md = crate::export::session_to_md(&agent.session);
                writeln!(out, "{}", json!({"type":"export_result","markdown":md}))?;
            }
            "quit" => break,
            "echo" => {
                writeln!(out, "{}", json!({"type":"echo","data":req.get("data").cloned().unwrap_or(json!(null))}))?;
            }
            unknown => {
                writeln!(out, "{}", json!({"type":"error","message":format!("unknown type '{unknown}'")}))?;
            }
        }
        out.flush()?;
    }
    writeln!(out, "{}", json!({"type":"bye","usage":{"input":agent.total_usage.input,"output":agent.total_usage.output,"cost":agent.total_cost}}))?;
    out.flush()?;
    let _ = toolset;
    let _ = cli;
    Ok(())
}

async fn run_prompt_parts(agent: &mut Agent, id: serde_json::Value, parts: Vec<pi_ai::types::ContentPart>, out: &mut std::io::BufWriter<std::io::Stdout>) {
    let mut rx = agent.user_turn_parts(parts).await;
    while let Some(ev) = rx.recv().await {
        let val: serde_json::Value = serde_json::to_value(&ev).unwrap_or(json!(null));
        let _ = writeln!(out, "{}", json!({"id":id,"type":"event","event":val}));
        if let AgentEvent::Error(_) = ev {
            break;
        }
    }
    let _ = writeln!(
        out,
        "{}",
        json!({"id":id,"type":"done","usage":{"input":agent.total_usage.input,"output":agent.total_usage.output,"cost":agent.total_cost}})
    );
}
