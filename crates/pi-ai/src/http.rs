//! Minimal synchronous HTTP + SSE helper on top of ureq 2.x (zero async deps
//! here; the agent crate bridges to tokio via spawn_blocking).

use std::io::BufRead;
use std::time::Duration;

use ureq::AgentBuilder;

use crate::types::ApiError;

pub fn make_agent() -> ureq::Agent {
    AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        // per-read stall timeout: catches dead streams without capping total
        .timeout_read(Duration::from_secs(120))
        .user_agent(concat!("pi-rs/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Retry a POST. Retries on transport errors and 429/5xx, but stops retrying
/// once bytes have been emitted (`emitted`).
pub fn post_with_retry(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(String, String)],
    body: &str,
    emitted: &mut bool,
    max_attempts: u32,
) -> Result<ureq::Response, ApiError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut req = agent.post(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_string(body) {
            Ok(resp) => return Ok(resp),
            Err(ureq::Error::Status(code, resp)) if *emitted => {
                return Err(read_http_error(code, resp));
            }
            Err(ureq::Error::Status(code, _)) if !retryable(code) => {
                return Err(ApiError::Http {
                    status: code,
                    body: format!("HTTP {code}"),
                });
            }
            Err(ureq::Error::Status(_code, resp)) if attempt < max_attempts => {
                backoff(attempt, &resp);
                continue;
            }
            Err(ureq::Error::Status(code, resp)) => return Err(read_http_error(code, resp)),
            Err(ureq::Error::Transport(_)) if attempt < max_attempts && !*emitted => {
                backoff_attempt(attempt);
                continue;
            }
            Err(ureq::Error::Transport(t)) => {
                return Err(ApiError::Transport(t.to_string()))
            }
        }
    }
}

fn retryable(code: u16) -> bool {
    code == 429 || (500..600).contains(&code)
}

fn backoff(attempt: u32, resp: &ureq::Response) {
    if let Some(v) = resp.header("retry-after") {
        if let Ok(s) = v.trim().parse::<u64>() {
            std::thread::sleep(Duration::from_secs(s.min(60)));
            return;
        }
    }
    backoff_attempt(attempt)
}

fn backoff_attempt(attempt: u32) {
    let base = 500u64;
    let delay = base.saturating_mul(2u64.pow(attempt.saturating_sub(1).min(5)));
    std::thread::sleep(Duration::from_millis(delay.min(8000)));
}

fn read_http_error(code: u16, resp: ureq::Response) -> ApiError {
    let mut r = std::io::BufReader::new(resp.into_reader());
    let mut buf = [0u8; 4096];
    let n = std::io::Read::read(&mut r, &mut buf).unwrap_or(0);
    let body = String::from_utf8_lossy(&buf[..n]).to_string();
    ApiError::Http {
        status: code,
        body: body.lines().take(8).collect::<Vec<_>>().join(" "),
    }
}

/// Stream an SSE response: call `on_sse(event, data)` for each complete frame.
/// `event` is the `event:` field or "message".
pub fn for_each_sse(
    resp: ureq::Response,
    on_sse: &mut dyn FnMut(&str, &str) -> Result<(), ApiError>,
) -> Result<(), ApiError> {
    let mut reader = std::io::BufReader::new(resp.into_reader());
    let mut event: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| ApiError::Stream(format!("read: {e}")))?;
        if n == 0 {
            break; // EOF
        }
        let raw = line.trim_end_matches(['\n', '\r']);
        if raw.is_empty() {
            if !data_lines.is_empty() {
                let data = data_lines.join("\n");
                let ev = event.take().unwrap_or_else(|| "message".into());
                match on_sse(&ev, &data) {
                    Ok(()) => {}
                    Err(ApiError::Aborted) => return Ok(()),
                    Err(e) => return Err(e),
                }
                data_lines.clear();
            }
            continue;
        }
        if let Some(rest) = raw.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        } else if let Some(rest) = raw.strip_prefix("event:") {
            event = Some(rest.trim_start().to_string());
        } else if let Some(rest) = raw.strip_prefix("id:") {
            let _ = rest; // ignored
        }
        // `:` comments and unknown fields are skipped
    }
    if !data_lines.is_empty() {
        let data = data_lines.join("\n");
        let ev = event.take().unwrap_or_else(|| "message".into());
        match on_sse(&ev, &data) {
            Ok(()) => {}
            Err(ApiError::Aborted) => return Ok(()),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
