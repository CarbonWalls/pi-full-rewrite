//! # pi-agent
//!
//! Agent runtime (Rust rewrite of `@earendil-works/pi-agent-core`):
//! streaming LLM loop with tool calling, JSONL sessions with tree branching,
//! automatic context compaction, and a typed event stream for UIs.

pub mod session;
pub mod tools;

use pi_ai::types::{
    ChatMessage, ChatRequest, ContentPart, Role, StopReason, StreamEvent, ThinkingLevel,
    ToolCall, Usage,
};
use pi_ai::{Llm, ModelInfo};
use std::sync::Arc;
use tokio::sync::mpsc;

use session::Session;
use tools::{ToolRegistry, ToolResult};

/// Events the agent emits while running a user turn.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// The user message we're answering.
    UserMessage(ChatMessage),
    /// Start of a new assistant message.
    AssistantStart { model: String },
    TextDelta(String),
    ReasoningDelta(String),
    /// A tool call was requested (args complete).
    ToolCallStart(ToolCall),
    /// A tool finished.
    ToolResult {
        call: ToolCall,
        result: ToolResult,
    },
    /// Complete assistant message (after the LLM finished).
    AssistantMessage {
        message: ChatMessage,
        stop_reason: StopReason,
    },
    Usage(Usage),
    /// Context was compacted.
    Compacted { summary: String },
    Error(String),
}

/// Explicit JSON shape (tagged newtype variants can't be serde-derived).
impl serde::Serialize for AgentEvent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde_json::json;
        let v = match self {
            AgentEvent::UserMessage(m) => json!({"type":"user","message":m}),
            AgentEvent::AssistantStart { model } => json!({"type":"assistant_start","model":model}),
            AgentEvent::TextDelta(t) => json!({"type":"text","delta":t}),
            AgentEvent::ReasoningDelta(t) => json!({"type":"reasoning","delta":t}),
            AgentEvent::ToolCallStart(c) => json!({"type":"tool_call","call":c}),
            AgentEvent::ToolResult { call, result } => json!({"type":"tool_result","call":call,"result":result}),
            AgentEvent::AssistantMessage { message, stop_reason } => json!({"type":"assistant","message":message,"stop_reason":stop_reason}),
            AgentEvent::Usage(u) => json!({"type":"usage","usage":u}),
            AgentEvent::Compacted { summary } => json!({"type":"compacted","summary":summary}),
            AgentEvent::Error(e) => json!({"type":"error","message":e}),
        };
        v.serialize(s)
    }
}

/// Configuration for an agent.
#[derive(Clone)]
pub struct AgentConfig {
    pub model: ModelInfo,
    /// Auto-switch to this model when the primary fails (transport errors,
    /// auth, rate limits).
    pub fallback_model: Option<ModelInfo>,
    pub system: Vec<ContentPart>,
    pub thinking: ThinkingLevel,
    /// Temperature (None = provider default).
    pub temperature: Option<f32>,
    /// Above this fraction of the context window we auto-compact.
    pub compact_threshold: f64,
    /// Maximum assistant turns per user turn (safety cap).
    pub max_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: ModelInfo {
                id: "echo".into(),
                provider: "mock".into(),
                name: "Mock Echo".into(),
                context_window: 128_000,
                max_output_tokens: 8_192,
                input_price: 0.0,
                output_price: 0.0,
                reasoning: false,
                tools: true,
                vision: false,
                api: Some("mock".into()),
                base_url: None,
                sampling: None,
            },
            fallback_model: None,
            system: Vec::new(),
            thinking: ThinkingLevel::Off,
            temperature: None,
            compact_threshold: 0.85,
            max_turns: 50,
        }
    }
}

/// The agent: owns config, tools, session, and the LLM client.
pub struct Agent {
    pub llm: Llm,
    pub config: AgentConfig,
    pub tools: ToolRegistry,
    pub session: Session,
    /// Cumulative tokens for this session.
    pub total_usage: Usage,
    /// Cumulative cost for this session (USD).
    pub total_cost: f64,
    /// Per-model usage (provider/model -> tokens), for the cost dashboard.
    pub model_usage: std::collections::BTreeMap<String, Usage>,
    /// Per-model cost in USD.
    pub model_cost: std::collections::BTreeMap<String, f64>,
    /// Cooperative abort: set true to stop the in-flight turn at the next
    /// streamed event. Checked by the UI on Ctrl-C.
    pub abort: Arc<std::sync::atomic::AtomicBool>,
    /// Where session files live (set by the CLI; used by serve's /cost).
    pub session_dir: Option<std::path::PathBuf>,
}

impl Agent {
    pub fn new(llm: Llm, config: AgentConfig, tools: ToolRegistry, session: Session) -> Self {
        Self {
            llm,
            config,
            tools,
            session,
            total_usage: Usage::default(),
            total_cost: 0.0,
            model_usage: std::collections::BTreeMap::new(),
            model_cost: std::collections::BTreeMap::new(),
            abort: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            session_dir: None,
        }
    }

    /// Clear the cooperative abort flag (before starting a new turn).
    pub fn clear_abort(&self) {
        self.abort.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn with_session(mut self, session: Session) -> Self {
        self.session = session;
        self
    }

    /// Format a per-model cost/usage dashboard (for `/cost`).
    pub fn cost_dashboard(&self) -> String {
        if self.model_usage.is_empty() {
            return "no usage yet".into();
        }
        let mut out = String::new();
        out.push_str("| model | in | out | cache | $ |\n");
        out.push_str("|---|---|---|---|---|\n");
        for (k, u) in &self.model_usage {
            let cost = self
                .model_cost
                .get(k)
                .map(|c| format!("${c:.4}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "| {} | {} | {} | {} | {cost} |\n",
                k, u.input, u.output, u.cache_read
            ));
        }
        out.push_str(&format!(
            "| **total** | **{}** | **{}** | **{}** | **${:.4}** |\n",
            self.total_usage.input,
            self.total_usage.output,
            self.total_usage.cache_read,
            self.total_cost
        ));
        out.trim().to_string()
    }

    /// Append a user message (text only) and run the agent loop to completion.
    pub async fn user_turn(
        &mut self,
        input: impl Into<String>,
    ) -> mpsc::UnboundedReceiver<AgentEvent> {
        self.user_turn_parts(vec![pi_ai::types::ContentPart::Text {
            text: input.into(),
        }])
        .await
    }

    /// Append a user message built from content parts (text + images) and run
    /// the agent loop to completion. Streams events on the returned channel.
    ///
    /// Delivery uses an UNBOUNDED channel: the turn must never block on the
    /// caller's drain timing (the caller only receives `rx` once this future
    /// returns, so a bounded channel deadlocks on outputs larger than its
    /// capacity). The backlog is bounded by the model's max output tokens.
    pub async fn user_turn_parts(
        &mut self,
        parts: Vec<ContentPart>,
    ) -> mpsc::UnboundedReceiver<AgentEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let user_msg = ChatMessage {
            role: Role::User,
            content: parts,
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        };
        self.session.push_user_message(user_msg.clone());
        let _ = tx.send(AgentEvent::UserMessage(user_msg));

        let mut turns = 0;
        loop {
            turns += 1;
            let had_tools = match self.one_cycle(&tx).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(e));
                    break;
                }
            };
            if !had_tools {
                break;
            }
            if turns >= self.config.max_turns || tx.is_closed() {
                if turns >= self.config.max_turns {
                    let _ = tx.send(AgentEvent::Error(
                        "stopped: max turns per request reached".into(),
                    ));
                }
                break;
            }
        }
        rx
    }

    /// Start a turn that runs in its own task (for UIs that must keep
    /// responding to input while the agent works). Ownership of the agent is
    /// handed to the task; retrieve it back via [`Turn::take_agent`] after the
    /// event stream closes.
    pub fn start_turn(self, input: impl Into<String>) -> Turn {
        let input = input.into();
        let (fwd_tx, rx) = mpsc::channel(512);
        let (agent_tx, agent_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut agent = self;
            let mut rx2 = agent.user_turn(input).await;
            while let Some(ev) = rx2.recv().await {
                if fwd_tx.send(ev).await.is_err() {
                    break;
                }
            }
            let _ = agent_tx.send(agent);
        });
        Turn {
            rx,
            agent_rx,
            done: false,
        }
    }
}

/// A running agent turn owned by a background task.
pub struct Turn {
    rx: mpsc::Receiver<AgentEvent>,
    agent_rx: tokio::sync::oneshot::Receiver<Agent>,
    done: bool,
}

impl Turn {
    /// Poll one pending event (non-blocking).
    pub fn try_recv(&mut self) -> Option<AgentEvent> {
        self.rx.try_recv().ok()
    }

    /// Await one event (blocks until the next event or the turn ends).
    pub async fn recv(&mut self) -> Option<AgentEvent> {
        self.rx.recv().await
    }

    /// True when the turn has finished and no more events will arrive.
    pub fn closed(&self) -> bool {
        self.done || self.rx.is_closed()
    }

    /// Try to take back the agent (only succeeds once the turn is finished).
    pub fn take_agent(&mut self) -> Option<Agent> {
        if !self.closed() {
            return None;
        }
        self.agent_rx.try_recv().ok()
    }

    /// Wait for the turn to finish and take back the agent (requires a runtime).
    pub async fn finish(mut self) -> Agent {
        loop {
            if let Some(a) = self.take_agent() {
                return a;
            }
            // consume remaining events
            let _ = self.rx.recv().await;
            tokio::task::yield_now().await;
        }
    }
}

impl Agent {
    /// One LLM round-trip plus tool execution. Returns whether tools ran
    /// (i.e. the loop should continue).
    async fn one_cycle(&mut self, tx: &mpsc::UnboundedSender<AgentEvent>) -> Result<bool, String> {
        let messages = self.session.messages();
        let req = ChatRequest {
            model: self.config.model.clone(),
            system: self.config.system.clone(),
            messages,
            tools: self.tools.specs(),
            tool_choice: pi_ai::types::ToolChoice::Auto,
            temperature: self.config.temperature,
            top_p: None,
            max_tokens: Some(self.config.model.max_output_tokens.min(u32::MAX as u64) as u32),
            thinking: self.config.thinking,
            stop: Vec::new(),
            seed: None,
            fallback_model: self.config.fallback_model.clone(),
        };

        // Bridge the synchronous provider stream onto our async task.
        let llm = self.llm.clone();
        let abort_in = self.abort.clone();
        let (ev_tx, mut ev_rx) = mpsc::channel::<StreamEvent>(256);
        let block = tokio::task::spawn_blocking(move || {
            llm.stream(&req, |ev| {
                // cooperative abort (Ctrl-C in the TUI)
                if abort_in.load(std::sync::atomic::Ordering::Relaxed) {
                    return false;
                }
                ev_tx.blocking_send(ev).is_ok()
            })
        });

        let mut had_tools = false;
        let mut stream_err: Option<String> = None;
        while let Some(ev) = ev_rx.recv().await {
            match ev {
                StreamEvent::Start { model } => {
                    let _ = tx.send(AgentEvent::AssistantStart { model });
                }
                StreamEvent::TextDelta(t) => {
                    let _ = tx.send(AgentEvent::TextDelta(t));
                }
                StreamEvent::ReasoningDelta(t) => {
                    let _ = tx.send(AgentEvent::ReasoningDelta(t));
                }
                StreamEvent::ToolDelta { .. } => {}
                StreamEvent::Usage(u) => {
                    let _ = tx.send(AgentEvent::Usage(u));
                }
                StreamEvent::Done {
                    message,
                    stop_reason,
                    usage,
                } => {
                    self.total_usage.input += usage.input;
                    self.total_usage.output += usage.output;
                    self.total_usage.cache_read += usage.cache_read;
                    self.total_usage.reasoning += usage.reasoning;
                    self.total_cost += self.llm.catalog.cost_usd(&self.config.model, &usage);
                    // per-model dashboard
                    let key = format!(
                        "{}/{}",
                        self.config.model.provider, self.config.model.id
                    );
                    let cost = self.llm.catalog.cost_usd(&self.config.model, &usage);
                    let entry = self.model_usage.entry(key.clone()).or_default();
                    entry.input += usage.input;
                    entry.output += usage.output;
                    entry.cache_read += usage.cache_read;
                    entry.reasoning += usage.reasoning;
                    let _ = tx.send(AgentEvent::AssistantMessage {
                        message: message.clone(),
                        stop_reason,
                    });
                    self.session.push_message(message.clone());
                    self.session.push_usage(usage.input, usage.output, cost, key.clone());
                    *self.model_cost.entry(key).or_insert(0.0) += cost;
                    had_tools = !message.tool_calls.is_empty();
                    if had_tools {
                        self.run_tools(message.tool_calls.clone(), tx).await;
                    } else if stop_reason == StopReason::Length {
                        // hit output cap: compact keeps context manageable
                        self.auto_compact(tx).await;
                    }
                }
                StreamEvent::Error(e) => {
                    stream_err = Some(e);
                    break;
                }
            }
            if tx.is_closed() {
                break; // consumer gone; stop burning tokens
            }
        }
        // Ensure the blocking stream task has settled before we report.
        let blocked = block.await;
        if stream_err.is_none() {
            match blocked {
                Ok(Ok(_)) => {}
                Ok(Err(api_err)) => {
                    stream_err = Some(api_err.to_string());
                }
                Err(join) => {
                    stream_err = Some(format!("stream task failed: {join}"));
                }
            }
        }
        if let Some(e) = stream_err {
            let _ = tx.send(AgentEvent::Error(e.clone()));
            return Err(e);
        }
        Ok(had_tools)
    }

    /// Execute tool calls in parallel; persist results as tool-role messages.
    async fn run_tools(&mut self, calls: Vec<ToolCall>, tx: &mpsc::UnboundedSender<AgentEvent>) {
        for call in &calls {
            let _ = tx.send(AgentEvent::ToolCallStart(call.clone()));
        }
        let registry: ToolRegistry = self.tools.clone();
        let mut results: Vec<ToolResult> = Vec::with_capacity(calls.len());
        let mut handles = Vec::with_capacity(calls.len());
        for call in &calls {
            let reg = registry.clone();
            let call = call.clone();
            handles.push(tokio::task::spawn_blocking(move || reg.execute(&call)));
        }
        for h in handles {
            results.push(match h.await {
                Ok(r) => r,
                Err(join) => ToolResult::err(format!("tool task failed: {join}")),
            });
        }
        for (call, result) in calls.iter().zip(results.iter()) {
            let _ = tx.send(AgentEvent::ToolResult {
                call: call.clone(),
                result: result.clone(),
            });
            let mut tool_msg = ChatMessage {
                role: Role::Tool,
                content: vec![ContentPart::Text {
                    text: result.output.clone(),
                }],
                tool_calls: Vec::new(),
                tool_call_id: Some(call.id.clone()),
                tool_name: Some(call.name.clone()),
            };
            if result.is_error {
                tool_msg.content.insert(
                    0,
                    ContentPart::Text {
                        text: "ERROR: ".into(),
                    },
                );
            }
            self.session.push_message(tool_msg);
        }
    }

    /// Summarize + compact when the context estimate crosses the threshold.
    async fn auto_compact(&mut self, tx: &mpsc::UnboundedSender<AgentEvent>) {
        let est = estimate_tokens(&self.session.messages());
        let limit = self.config.model.context_window;
        if limit == 0
            || (est as f64) < (limit as f64 * self.config.compact_threshold)
        {
            return;
        }
        let messages = self.session.messages();
        let summary = self.summarize(&messages).await;
        self.session.push_compaction(summary.clone());
        let _ = tx.send(AgentEvent::Compacted {
            summary: summary.clone(),
        });
        let _ = summary;
    }

    /// Explicit compaction (`/compact`): always summarizes the current branch.
    pub async fn compact(&mut self, tx: &mpsc::UnboundedSender<AgentEvent>) {
        let messages = self.session.messages();
        if messages.len() < 2 {
            return;
        }
        let summary = self.summarize(&messages).await;
        self.session.push_compaction(summary.clone());
        let _ = tx.send(AgentEvent::Compacted {
            summary: summary.clone(),
        });
    }

    /// Compact and swallow the events (convenience for UIs).
    pub async fn compact_now(&mut self) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.compact(&tx).await;
        while rx.try_recv().is_ok() {}
    }

    async fn summarize(&self, messages: &[ChatMessage]) -> String {
        let req = ChatRequest {
            model: self.config.model.clone(),
            system: vec![ContentPart::Text {
                text: "Summarize this conversation concisely. Preserve key decisions, file names, and outstanding tasks. Answer with plain text only.".into(),
            }],
            messages: messages.to_vec(),
            tools: Vec::new(),
            tool_choice: pi_ai::types::ToolChoice::None,
            temperature: Some(0.2),
            top_p: None,
            max_tokens: Some(2048),
            thinking: ThinkingLevel::Off,
            stop: Vec::new(),
            seed: None,
            fallback_model: self.config.fallback_model.clone(),
        };
        let llm = self.llm.clone();
        tokio::task::spawn_blocking(move || {
            let mut out = String::new();
            let _ = llm.stream(&req, |ev| {
                if let StreamEvent::TextDelta(t) = ev {
                    out.push_str(&t);
                } else if let StreamEvent::Error(_) = ev {
                    return false;
                }
                true
            });
            if out.trim().is_empty() {
                out = "[compaction: summarization unavailable]".to_string();
            }
            out
        })
        .await
        .unwrap_or_else(|_| "[compaction: task failed]".to_string())
    }
}

/// Cheap token estimate (~chars/4) used for auto-compact decisions.
pub fn estimate_tokens(msgs: &[ChatMessage]) -> usize {
    msgs.iter()
        .map(|m| m.text().len() / 4 + m.tool_calls.len() * 64)
        .sum()
}
