//! Provider dispatch.

pub mod anthropic;
pub mod gemini;
pub mod mock;
pub mod openai;

use crate::types::{ApiError, ChatRequest, StreamEvent};

/// Dispatch a streaming chat request to the right provider backend.
///
/// The callback is passed **by value** down the chain (never re-borrowed) so
/// the model-fallback path cannot nest `&mut &mut …` into infinite types.
pub fn stream_to(
    req: &ChatRequest,
    api_key: Option<&str>,
    on_event: impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    dispatch(req, api_key, on_event)
}

fn dispatch<F: FnMut(StreamEvent) -> bool>(
    req: &ChatRequest,
    api_key: Option<&str>,
    on_event: F,
) -> Result<(), ApiError> {
    if req.model.provider == "mock" {
        return mock::stream_to(req, on_event);
    }
    match req.model.resolved_api() {
        crate::types::Api::OpenAiCompat | crate::types::Api::Custom(_) => {
            openai::stream_to(req, api_key, on_event)
        }
        crate::types::Api::Anthropic => anthropic::stream_to(req, api_key, on_event),
        crate::types::Api::Gemini => gemini::stream_to(req, api_key, on_event),
        crate::types::Api::OpenAiResponses => {
            // Responses API mapped onto the compat path for now; the native
            // implementation lands in a later milestone.
            openai::stream_to(req, api_key, on_event)
        }
    }
}

/// Deterministic offline provider for tests and the `pi mock/echo` dev
/// backend. Echoes the last user message; with tools advertised and the token
/// `@tool` present, issues a call to the first advertised tool.
pub fn mock_stream_to(
    req: &ChatRequest,
    on_event: impl FnMut(StreamEvent) -> bool,
) -> Result<(), ApiError> {
    mock::stream_to(req, on_event)
}
