//! # pi-ai
//!
//! Unified multi-provider LLM API (Rust rewrite of `@earendil-works/pi-ai`).
//!
//! - Provider backends: OpenAI-compatible (OpenAI, OpenRouter, DeepSeek, Mistral,
//!   Groq, Ollama, vLLM, any custom base URL), Anthropic Messages, Google Gemini.
//! - Streaming event stream with incremental tool-call deltas.
//! - Model catalog: embedded + local JSON overlay.
//! - Key resolution: keys file → environment.
//! - Automatic model fallback on transport errors (extra over pi).

pub mod auth;
pub mod catalog;
pub mod http;
pub mod providers;
pub mod scenario;
pub mod types;

pub use types::{
    Api, ApiError, ChatMessage, ChatRequest, ContentPart, ImageSource, ModelInfo, Role,
    StopReason, StreamEvent, ThinkingLevel, ToolCall, ToolChoice, ToolSpec, Usage,
};
pub use auth::Keys;
pub use catalog::Catalog;

/// High-level facade over catalog + auth + provider dispatch.
#[derive(Clone)]
pub struct Llm {
    pub catalog: Catalog,
    pub keys: Keys,
}

impl Llm {
    pub fn new() -> Self {
        Self {
            catalog: Catalog::builtin().clone(),
            keys: Keys::from_env(),
        }
    }

    /// With overlay catalog + keys file.
    pub fn with(
        catalog: Catalog,
        keys: Keys,
    ) -> Self {
        Self { catalog, keys }
    }

    pub fn model(&self, refstr: &str) -> Result<ModelInfo, ApiError> {
        self.catalog
            .resolve(refstr)
            .ok_or_else(|| ApiError::Other(format!("unknown model '{refstr}'")))
    }

    /// Stream a chat completion; `api_key` may override key resolution.
    /// Runs synchronously — callers bridge to async with `spawn_blocking`.
    pub fn stream(
        &self,
        req: &ChatRequest,
        on_event: impl FnMut(StreamEvent) -> bool,
    ) -> Result<(), ApiError> {
        let key = self.keys.for_model_by_provider(&req.model.provider);
        providers::stream_to(req, key.as_deref(), on_event)
    }
}

impl Default for Llm {
    fn default() -> Self {
        Self::new()
    }
}

/// Small helper so `Llm::stream` can resolve keys by provider string.
impl Keys {
    pub fn for_model_by_provider(&self, provider: &str) -> Option<String> {
        if provider == "mock" {
            return Some("mock".into());
        }
        self.provider.get(provider).cloned()
    }
}
