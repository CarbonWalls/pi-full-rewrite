//! Core LLM types for pi-ai.
//!
//! Conceptual superset of `@earendil-works/pi-ai` message model:
//! provider-neutral chat messages, streaming events, tool specs.

use serde::{Deserialize, Serialize};

/// Provider-neutral LLM API families. Custom strings allow user-defined
/// backends via model catalog overlays (like pi's `pi-messages` compat api).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Api {
    /// OpenAI-compatible /chat/completions (OpenAI, OpenRouter, DeepSeek, Ollama, vLLM, …)
    OpenAiCompat,
    /// OpenAI /responses (experimental)
    OpenAiResponses,
    /// Anthropic /v1/messages
    Anthropic,
    /// Google generateContent
    Gemini,
    /// User-defined / experimental, dispatched by the model's `api` string in catalog overlays
    Custom(String),
}

impl Api {
    pub fn from_str_api(s: &str) -> Self {
        match s {
            "openai-completions" | "openai-compatible" | "mistral-conversations" => {
                Api::OpenAiCompat
            }
            "openai-responses" | "azure-openai-responses" | "openai-codex-responses" => {
                Api::OpenAiResponses
            }
            "anthropic-messages" => Api::Anthropic,
            "google-generative-ai" | "google-vertex" => Api::Gemini,
            "pi-messages" => Api::Custom("pi".into()),
            _ => Api::Custom(s.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Api::OpenAiCompat => "openai-completions",
            Api::OpenAiResponses => "openai-responses",
            Api::Anthropic => "anthropic-messages",
            Api::Gemini => "google-generative-ai",
            Api::Custom(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSource {
    /// data URI body (no `data:` prefix)
    Base64(String),
    Url(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentPart {
    Text { text: String },
    Image {
        media_type: String,
        source: ImageSource,
    },
    /// model reasoning/thinking content (kept verbatim, stripped per provider)
    Reasoning { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments as emitted by the model (may be partial in deltas).
    pub arguments: serde_json::Value,
}

impl ToolCall {
    pub fn argument_str(&self) -> String {
        match &self.arguments {
            serde_json::Value::String(s) => s.clone(),
            v => v.to_string(),
        }
    }
}

/// Provider-neutral chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(default)]
    pub content: Vec<ContentPart>,
    /// assistant-only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// tool-role only
    pub tool_call_id: Option<String>,
    /// tool-role: which tool produced this result
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn text_user(msg: &str) -> Self {
        ChatMessage {
            role: Role::User,
            content: vec![ContentPart::Text { text: msg.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            tool_name: None,
        }
    }

    pub fn text_assistant(msg: &str) -> Self {
        ChatMessage {
            role: Role::Assistant,
            content: vec![ContentPart::Text { text: msg.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            tool_name: None,
        }
    }

    pub fn system(msg: &str) -> Self {
        ChatMessage {
            role: Role::System,
            content: vec![ContentPart::Text { text: msg.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            tool_name: None,
        }
    }
}

/// Tool spec advertised to the model (JSON-Schema `parameters`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    #[default]
    Stop,
    ToolCalls,
    Length,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    /// cached input tokens (prompt cache reads)
    pub cache_read: u64,
    /// tokens written to cache
    pub cache_write: u64,
    pub reasoning: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.reasoning
    }
}

/// Model metadata (mirrors pi's `Model` catalog entry, trimmed to essentials).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    /// Display name
    pub name: String,
    #[serde(default = "default_context")]
    pub context_window: u64,
    #[serde(default = "default_max_out")]
    pub max_output_tokens: u64,
    /// USD per 1M tokens
    #[serde(default)]
    pub input_price: f64,
    #[serde(default)]
    pub output_price: f64,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub vision: bool,
    /// API family; default depends on provider (see [`Api::from_str_api`]).
    pub api: Option<String>,
    /// Custom base URL for OpenAI-compatible backends (or any provider override).
    pub base_url: Option<String>,
    /// Sampling params merged per thinking level (`low`/`medium`/`high`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<serde_json::Value>,
}

impl ModelInfo {
    pub fn resolved_api(&self) -> Api {
        self.api
            .as_deref()
            .map(Api::from_str_api)
            .unwrap_or_else(|| match self.provider.as_str() {
                "anthropic" => Api::Anthropic,
                "google" => Api::Gemini,
                _ => Api::OpenAiCompat,
            })
    }

    pub fn base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| default_base_url(&self.provider))
    }
}

fn default_context() -> u64 {
    128_000
}
fn default_max_out() -> u64 {
    8_192
}

pub fn default_base_url(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1".into(),
        "openrouter" => "https://openrouter.ai/api/v1".into(),
        "deepseek" => "https://api.deepseek.com/v1".into(),
        "mistral" => "https://api.mistral.ai/v1".into(),
        "groq" => "https://api.groq.com/v1/openai".into(),
        "cerebras" => "https://api.cerebras.ai/v1".into(),
        "xai" => "https://api.x.ai/v1".into(),
        "anthropic" => "https://api.anthropic.com/v1".into(),
        "google" => "https://generativelanguage.googleapis.com/v1beta".into(),
        "zai" | "moonshotai" => "https://api.moonshot.cn/v1".into(),
        "kimi-coding" => "https://api.moonshot.cn/v1".into(),
        "qwen" => "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
        "ollama" => "http://localhost:11434/v1".into(),
        "vllm" | "lmstudio" => "http://localhost:8000/v1".into(),
        _ => format!("https://api.{provider}/v1"),
    }
}

/// One streaming request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: ModelInfo,
    pub system: Vec<ContentPart>,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub thinking: ThinkingLevel,
    pub stop: Vec<String>,
    pub seed: Option<u64>,
    /// Automatic fallback when the primary model fails (extra over pi).
    pub fallback_model: Option<ModelInfo>,
}

/// Stream events emitted by any provider, in order.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Start { model: String },
    TextDelta(String),
    ReasoningDelta(String),
    /// Incremental tool-call updates; `index` is the position in the tool-call list.
    ToolDelta {
        index: u32,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    Usage(Usage),
    /// Final: assembled assistant message.
    Done {
        message: ChatMessage,
        stop_reason: StopReason,
        usage: Usage,
    },
    Error(String),
}

/// Stream a chat completion synchronously. `on_event` is called for each event;
/// returning `false` aborts the stream early.
pub type StreamFn<'a> = &'a mut dyn FnMut(StreamEvent) -> bool;

/// Error type for the whole crate.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("api key missing for provider '{0}'")]
    MissingKey(String),
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("stream error: {0}")]
    Stream(String),
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    /// Consumer requested early abort (cancellation); not a real error.
    #[error("stream aborted")]
    Aborted,
    #[error("{0}")]
    Other(String),
}
