//! `pi serve`: HTTP + JSON-lines RPC over TCP.
//!
//! Routes (all on one port):
//!   GET  /healthz  -> {"ok":true}
//!   GET  /info     -> {model, tools, usage}
//!   POST /rpc      -> body: one JSON request per line; response: one JSON
//!                      line per event/result + a `done` line.
//!
//! The agent is serialized behind a tokio mutex: one prompt at a time,
//! matching `--rpc` semantics.

use pi_agent::Agent;
use serde_json::json;
use std::sync::Arc;
pub const DASHBOARD_HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>pi</title>
<style>
body{font-family:system-ui,sans-serif;max-width:900px;margin:24px auto;padding:0 16px;background:#0f1117;color:#e6e6e6}
h1{font-size:20px}pre{background:#161922;border:1px solid #2a2e3d;padding:10px;white-space:pre-wrap;max-height:50vh;overflow:auto}
.row{display:flex;gap:8px;margin:10px 0}input{flex:1;padding:8px;background:#161922;border:1px solid #2a2e3d;color:#e6e6e6}
button{padding:8px 14px} .m{font-size:12px;color:#9aa} .tag{color:#8a63d2;font-weight:600}
#log .u{color:#0b93f6} #log .a{color:#22b573}
</style></head><body>
<h1>pi <span class="m" id="meta"></span></h1>
<div class="row"><input id="prompt" placeholder="ask pi..." onkeydown="if(event.key==='Enter')run()">
<button onclick="run()">send</button> <button onclick="stop()">&#9632; stop</button></div>
<pre id="log">(no turns yet)</pre>
<h3>sessions</h3><pre id="sessions">…</pre>
<h3>cost</h3><pre id="cost">…</pre>
<h3>metrics</h3><pre id="metrics">…</pre>
<script>
async function meta(){ try{const r=await fetch('/info');const d=await r.json();
document.getElementById('meta').textContent='· '+d.service+' '+d.version+' · session '+(d.session_id||'')}catch(e){} }
async function sessions(){ try{const r=await fetch('/sessions');const d=await r.json();
  const el=document.getElementById('sessions');
  if(!d.sessions||!d.sessions.length){el.textContent='(none)';return}
  el.textContent=d.sessions.map(s=>s.id.slice(-8)+' · '+s.messages+' msgs').join('\n');
}catch(e){} }
async function cost(){ try{const r=await fetch('/cost');const d=await r.json();
  document.getElementById('cost').textContent=
    'calls: '+d.model_calls+' · in='+d.total.input+' out='+d.total.output+' cost=$'+d.total.cost.toFixed(6);
}catch(e){} }
async function metrics(){ try{const r=await fetch('/metrics');document.getElementById('metrics').textContent=await r.text()}catch(e){} }
async function stop(){
  await fetch('/rpc',{method:'POST',body:JSON.stringify({id:Date.now(),type:'abort'})});
  const log=document.getElementById('log');
  log.appendChild(Object.assign(document.createElement('div'),{className:'tag',textContent:'■ aborted'}));
}
async function run(){
  const p=document.getElementById('prompt'); const log=document.getElementById('log');
  if(!p.value.trim())return;
  if(log.textContent==='(no turns yet)') log.textContent='';
  log.appendChild(Object.assign(document.createElement('div'),{className:'u',textContent:'▶ '+p.value}));
  const body=JSON.stringify({id:Date.now(),type:'prompt',input:p.value});
  p.value='';
  const resp=await fetch('/rpc',{method:'POST',body});
  const reader=resp.body.getReader(); const dec=new TextDecoder(); let buf='';
  for(;;){ const {done,value}=await reader.read(); if(done)break;
    buf+=dec.decode(value,{stream:true});
    let i; while((i=buf.indexOf('\n'))>=0){ const line=buf.slice(0,i); buf=buf.slice(i+1); if(!line.trim())continue;
      let ev; try{ev=JSON.parse(line)}catch(e){continue}
      if(ev.type==='event'){ const e=ev.event;
        if(e.type==='text') log.appendChild(Object.assign(document.createElement('div'),{className:'a',textContent:e.delta}));
        else if(e.type==='tool_call_start') log.appendChild(Object.assign(document.createElement('div'),{className:'tag',textContent:'⚙ '+e.name}));
      }
    }
  }
  log.scrollTop=log.scrollHeight;
}
meta(); sessions(); cost(); metrics(); setInterval(metrics,3000); setInterval(sessions,5000);
</script></body></html>
"#;

use tokio::net::TcpListener;
use tokio::sync::Mutex;

pub async fn serve(
    agent: Agent,
    addr: String,
    port: u16,
    token: Option<String>,
    cors: Option<String>,
    access_log: bool,
    session_dir: std::path::PathBuf,
) -> anyhow::Result<()> {
    let listener = bind_listener(&addr, port).await?;
    let bind = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{addr}:{port}"));
    serve_listener(listener, agent, token, bind, cors, access_log, session_dir).await
}

/// Lock-free /info mirror: the static bits are snapshotted once at startup
/// and usage/cost counters are updated by the prompt arm, so /info never
/// waits on the agent mutex while a turn runs.
pub struct SharedInfo {
    pub model: String,
    pub tools: Vec<String>,
    pub session_id: String,
    pub session_dir: std::path::PathBuf,
    pub input: std::sync::atomic::AtomicU64,
    pub output: std::sync::atomic::AtomicU64,
    pub cost: std::sync::atomic::AtomicU64,
}

/// A capped in-memory access log (last N request lines), exposed via
/// `/access-log` when enabled.
pub struct AccessLog {
    lines: std::sync::Mutex<std::collections::VecDeque<String>>,
    cap: usize,
}

impl AccessLog {
    pub fn new(cap: usize) -> Self {
        Self { lines: std::sync::Mutex::new(std::collections::VecDeque::new()), cap }
    }
    pub fn push(&self, line: String) {
        let mut l = self.lines.lock().unwrap();
        if l.len() >= self.cap {
            l.pop_front();
        }
        l.push_back(line);
    }
    pub fn push_req(&self, method: &str, path: &str, code: u16) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        // cap the logged path so a hostile 10 MB URL can't balloon the log
        let p: String = path.chars().take(400).collect();
        let p = if path.len() > 400 { format!("{p}…") } else { p };
        self.push(format!("{code} {method} {p} @ {now}ms"));
    }
    pub fn dump(&self) -> String {
        self.lines.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n") + "\n"
    }
}

/// Simple Prometheus-style counters for the serve process.
#[derive(Default)]
pub struct Metrics {
    pub requests: std::sync::atomic::AtomicU64,
    pub rpc_prompts: std::sync::atomic::AtomicU64,
    pub tools_run: std::sync::atomic::AtomicU64,
    pub auth_failures: std::sync::atomic::AtomicU64,
    pub errors: std::sync::atomic::AtomicU64,
}

/// Format the counters as text/plain (Prometheus exposition format).
pub fn render_metrics(m: &Metrics) -> String {
    let g = |name: &str, v: u64| format!("pi_{name} {v}\n");
    let mut out = String::from("# TYPE pi_requests counter\n");
    out.push_str(&g("requests", m.requests.load(std::sync::atomic::Ordering::Relaxed)));
    out.push_str("# TYPE pi_rpc_prompts counter\n");
    out.push_str(&g("rpc_prompts", m.rpc_prompts.load(std::sync::atomic::Ordering::Relaxed)));
    out.push_str("# TYPE pi_tools_run counter\n");
    out.push_str(&g("tools_run", m.tools_run.load(std::sync::atomic::Ordering::Relaxed)));
    out.push_str("# TYPE pi_auth_failures counter\n");
    out.push_str(&g("auth_failures", m.auth_failures.load(std::sync::atomic::Ordering::Relaxed)));
    out.push_str("# TYPE pi_errors counter\n");
    out.push_str(&g("errors", m.errors.load(std::sync::atomic::Ordering::Relaxed)));
    out
}

/// Bind a listener; with `port = 0` the OS picks a free port (tests).
pub async fn bind_listener(addr: &str, port: u16) -> anyhow::Result<TcpListener> {
    let bind = if addr.contains(':') {
        addr.to_string()
    } else {
        format!("{addr}:{port}")
    };
    Ok(TcpListener::bind(&bind).await?)
}

/// Serve on an already-bound listener (used by `serve` and by tests).
pub async fn serve_listener(
    listener: TcpListener,
    agent: Agent,
    token: Option<String>,
    bind: String,
    cors: Option<String>,
    access_log: bool,
    session_dir: std::path::PathBuf,
) -> anyhow::Result<()> {
    if token.is_some() {
        eprintln!(
            "pi serve: listening on {bind} (auth required via Authorization: Bearer <token>) — Ctrl-C to stop"
        );
    } else {
        eprintln!(
            "pi serve: listening on {bind} (GET /healthz /info /sessions, POST /rpc) — Ctrl-C to stop"
        );
    }
    let shared: Arc<Mutex<Agent>> = Arc::new(Mutex::new(agent));
    let session_dir = Arc::new(session_dir);
    // external abort handle + lock-free /info snapshot (one brief lock)
    let abort = shared.lock().await.abort.clone();
    let info: SharedInfo = {
        let g = shared.lock().await;
        SharedInfo {
            model: format!("{}/{}", g.config.model.provider, g.config.model.id),
            tools: g.tools.names(),
            session_id: g.session.id.clone(),
            session_dir: (*session_dir).clone(),
            input: std::sync::atomic::AtomicU64::new(g.total_usage.input),
            output: std::sync::atomic::AtomicU64::new(g.total_usage.output),
            cost: std::sync::atomic::AtomicU64::new((g.total_cost * 1e8) as u64),
        }
    };
    let info = Arc::new(info);
    // bound the number of concurrently-handled connections (audit finding)
    use tokio::sync::Semaphore;
    let conns = Arc::new(Semaphore::new(256));
    let metrics = Arc::new(Metrics::default());
    let alog = if access_log {
        Some(Arc::new(AccessLog::new(200)))
    } else {
        None
    };
    // SIGTERM -> graceful shutdown (supervisor/systemd friendly): a spawned
    // listener completes a oneshot the accept loop selects on.
    let (term_tx, mut term_rx) = tokio::sync::oneshot::channel::<()>();
    #[cfg(unix)]
    if let Ok(mut sig) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::spawn(async move {
            let _ = sig.recv().await;
            let _ = term_tx.send(());
        });
    }
    #[cfg(not(unix))]
    drop(term_tx);

    loop {
        // graceful shutdown on SIGINT and SIGTERM; returning drops the
        // listener.
        let accept = tokio::select! {
            res = listener.accept() => res,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("pi serve: shutdown (SIGINT)");
                return Ok(());
            }
            _ = &mut term_rx => {
                eprintln!("pi serve: shutdown (SIGTERM)");
                return Ok(());
            }
        };
        let (sock0, _) = match accept {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut sock = sock0;
        let agent = shared.clone();
        let token = token.clone();
        let cors = cors.clone();
        let alog_rx = alog.clone();
        let metrics = metrics.clone();
        let abort2 = abort.clone();
        let sd2 = session_dir.clone();
        let info2 = info.clone();
        let permit = match conns.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                // hard concurrency cap: refuse politely instead of spawning
                // unbounded handlers (audit finding).
                let _ = write_resp(&mut sock, 503, "too many connections", &cors).await;
                continue;
            }
        };
        tokio::spawn(async move {
            // hard cap on a single connection: a stalled client (huge
            // content-length, never-sent body) can't hold a handler forever.
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(120),
                handle_conn(sock, &agent, token.as_deref(), &metrics, cors.as_deref(), alog_rx, &abort2, sd2, info2),
            )
            .await;
            drop(permit); // free the slot when this handler finishes
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_conn(
    sock: tokio::net::TcpStream,
    agent: &Arc<Mutex<Agent>>,
    token: Option<&str>,
    metrics: &Metrics,
    cors: Option<&str>,
    alog: Option<Arc<AccessLog>>,
    abort: &Arc<std::sync::atomic::AtomicBool>,
    session_dir: Arc<std::path::PathBuf>,
    info: Arc<SharedInfo>,
) -> std::io::Result<()> {
    metrics.requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cors: Option<String> = cors.map(|c| c.to_string());
    use tokio::io::AsyncReadExt;
    let mut stream = sock;
    // read the full HTTP request. With a Content-Length we stop exactly at
    // the body end (audit: was off by 4 — the body starts at pos+4).
    // Without one we treat the body as empty (the historical contract; the
    // fuzz clients rely on it). One request per connection, so stalls are
    // bounded by the 120s connection cap.
    let mut head = Vec::new();
    let mut buf = [0u8; 8192];
    let mut expected: Option<usize> = None;
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        head.extend_from_slice(&buf[..n]);
        if expected.is_none() {
            if let Some(pos) = find_head_end(&head) {
                let body_start = pos + 4;
                expected = match content_length_of(&head[..pos]) {
                    Some(cl) => match cl.checked_add(body_start) {
                        Some(v) => Some(v),
                        None => break, // absurd length: bail out of the read
                    },
                    None => Some(body_start), // no Content-Length: no body
                };
            }
        }
        if let Some(need) = expected {
            if head.len() >= need {
                break;
            }
        }
        if head.len() > 10_000_000 {
            break; // absurd guard
        }
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let (first, rest) = match split_once_crlf(&text) {
        Some((l, r)) => (l, r),
        None => return write_resp(&mut stream, 400, "bad request", &cors).await,
    };
    let mut parts = first.split_whitespace();
    let mut method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    // HEAD shares GET semantics for routing; the handler omits the body.
    let is_head = method == "HEAD";
    if is_head {
        method = "GET";
    }
    // split headers from the actual request body
    let body_text = rest
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_string();

    // token auth (healthz stays open for liveness probes)
    if let Some(tok) = token {
        if path != "/healthz" {
            let headers = rest.split_once("\r\n\r\n").map(|(h, _)| h).unwrap_or(rest);
            let authorized = headers
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim())
                .map(|v| {
                    v.strip_prefix("Bearer ")
                        .map(|b| b.trim() == tok)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !authorized {
                let body = json!({"error":"unauthorized"}).to_string();
                let head = format!(
                    "HTTP/1.1 401 Unauthorized\r\n{cc}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len(),
                    cc = cors_header(&cors)
                );
                use tokio::io::AsyncWriteExt;
                stream.write_all(head.as_bytes()).await?;
                stream.write_all(body.as_bytes()).await?;
                if let Some(a) = &alog {
                    a.push_req(method, path, 401);
                }
                return Ok(());
            }
        }
    }

    // POST /rpc streams JSON lines as they arrive (no content-length; body
    // is delimited by connection close). Everything else is buffered.
    // CORS preflight
    if method == "OPTIONS" && cors.is_some() {
        let origin = cors.as_deref().unwrap_or_default();
        let head = format!(
            "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        use tokio::io::AsyncWriteExt;
        let mut s = stream;
        s.write_all(head.as_bytes()).await?;
        return Ok(());
    }

    if method == "POST" && path == "/rpc" {
        metrics.rpc_prompts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(a) = &alog {
            a.push_req(method, path, 200);
        }
        return stream_rpc(&mut stream, &body_text, agent, &cors, abort, &info).await;
    }

    if method == "GET" && path == "/access-log" {
        let body = match &alog {
            Some(a) => a.dump(),
            None => "(access log disabled — start serve with --access-log)\n".into(),
        };
        // NOTE: `\r\n{cc}Content-Type` (no `\r\n` after {cc}) — a stray
        // `\r\n` terminator with cors unset ends the headers right after the
        // status line and the real headers leak into the body.
        let head = format!(
            "HTTP/1.1 200 OK\r\n{cc}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len(),
            cc = cors_header(&cors)
        );
        use tokio::io::AsyncWriteExt;
        let mut s = stream;
        s.write_all(head.as_bytes()).await?;
        s.write_all(body.as_bytes()).await?;
        return Ok(());
    }

    if method == "GET" && path == "/metrics" {
        let body = render_metrics(metrics);
        let head = format!(
            "HTTP/1.1 200 OK\r\n{cc}Content-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len(),
            cc = cors_header(&cors)
        );
        use tokio::io::AsyncWriteExt;
        let mut s = stream;
        s.write_all(head.as_bytes()).await?;
        s.write_all(body.as_bytes()).await?;
        return Ok(());
    }

    let (code, body, mime) = route(method, path, &body_text, agent, &session_dir, &info).await;
    if let Some(a) = &alog {
        a.push_req(method, path, code);
    }
    if is_head && code == 200 {
        // headers only, matching the GET semantics
        let head = format!(
            "HTTP/1.1 {code} OK\r\n{cc}Content-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len(),
            cc = cors_header(&cors)
        );
        use tokio::io::AsyncWriteExt;
        stream.write_all(head.as_bytes()).await?;
        return Ok(());
    }
    write_resp_mime(&mut stream, code, &body, &cors, mime).await
}

/// Stream rpc events: header first, then one JSON line per event/result,
/// flushed immediately so long turns are visible in real time.
async fn stream_rpc(
    stream: &mut tokio::net::TcpStream,
    body: &str,
    agent: &Arc<Mutex<Agent>>,
    cors: &Option<String>,
    abort: &Arc<std::sync::atomic::AtomicBool>,
    info: &SharedInfo,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let head = format!(
        "HTTP/1.1 200 OK\r\n{cc}Content-Type: application/x-ndjson\r\nConnection: close\r\n\r\n",
        cc = cors_header(cors)
    );
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await?;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({"type":"error","message":format!("bad json: {e}")});
                stream.write_all(format!("{err}\n").as_bytes()).await?;
                continue;
            }
        };
        let rtype = req.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let id = req.get("id").cloned().unwrap_or(json!(null));
        // abort must not wait for the agent lock (the running prompt holds
        // it for its whole turn) — poke the flag and ack immediately.
        if rtype == "abort" {
            abort.store(true, std::sync::atomic::Ordering::Relaxed);
            let item = json!({"id": id, "type":"aborted"});
            stream.write_all(format!("{item}
").as_bytes()).await?;
            stream.flush().await?;
            continue;
        }
        let mut g = agent.lock().await;
        match rtype {
            "prompt" => {
                abort.store(false, std::sync::atomic::Ordering::Relaxed);
                let input = req
                    .get("input")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut rx = g.user_turn_parts(
                    serde_json::from_value(
                        req.get("parts")
                            .cloned()
                            .unwrap_or_else(|| json!([{"type":"text","text":input}])),
                    )
                    .unwrap_or_else(|_| {
                        vec![pi_ai::types::ContentPart::Text { text: input.clone() }]
                    }),
                )
                .await;
                while let Some(ev) = rx.recv().await {
                    let val = serde_json::to_value(ev)
                        .unwrap_or_else(|_| json!({"type":"error","message":"serialize"}));
                    let item = json!({"id": id, "type":"event", "event": val});
                    stream.write_all(format!("{item}\n").as_bytes()).await?;
                    stream.flush().await?;
                }
                let done = json!({"id": id, "type":"done", "usage":{
                    "input": g.total_usage.input,
                    "output": g.total_usage.output,
                    "cost": g.total_cost}});
                stream.write_all(format!("{done}\n").as_bytes()).await?;
                // keep the lock-free /info mirror current
                info.input.store(g.total_usage.input, std::sync::atomic::Ordering::Relaxed);
                info.output.store(g.total_usage.output, std::sync::atomic::Ordering::Relaxed);
                info.cost.store((g.total_cost * 1e8) as u64, std::sync::atomic::Ordering::Relaxed);
                // operator console: one summary line per completed turn
                eprintln!(
                    "[turn] model={} in={} out={} cost=${:.6}",
                    g.config.model.provider,
                    g.total_usage.input,
                    g.total_usage.output,
                    g.total_cost
                );
            }
            "tools" => {
                let item = json!({"type":"tools_result","tools":g.tools.names()});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
            "model" => {
                let refstr = req.get("ref").and_then(|v| v.as_str()).unwrap_or("");
                let ok = if refstr.is_empty() {
                    true
                } else {
                    match g.llm.model(refstr) {
                        Ok(m) => {
                            g.config.model = m;
                            true
                        }
                        Err(e) => {
                            let item = json!({"type":"error","message":format!("model: {e}")});
                            stream.write_all(format!("{item}\n").as_bytes()).await?;
                            false
                        }
                    }
                };
                let item = json!({"type":"model_result","ok":ok,"model":g.config.model.id});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
            "compact" => {
                g.compact_now().await;
                let item = json!({"type":"compact_result","ok":true});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
            "usage" => {
                let item = json!({"type":"usage","input":g.total_usage.input,"output":g.total_usage.output,"cost":g.total_cost});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
            "session" => {
                let item = json!({"type":"session_result","id":g.session.id});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
            "quit" => return Ok(()),
            other => {
                let item = json!({"type":"error","message":format!("unknown type '{other}'")});
                stream.write_all(format!("{item}\n").as_bytes()).await?;
            }
        }
        stream.flush().await?;
    }
    Ok(())
}

async fn route(
    method: &str,
    path: &str,
    _body: &str,
    _agent: &Arc<Mutex<Agent>>,
    session_dir: &std::path::Path,
    info: &SharedInfo,
) -> (u16, String, &'static str) {
    match (method, path) {
        ("GET", "/") | ("GET", "/dashboard") => {
            (200, DASHBOARD_HTML.to_string(), "text/html")
        }
        ("GET", "/healthz") => (
            200,
            json!({"ok":true,"service":"pi"}).to_string(),
            "application/json",
        ),
        ("GET", "/info") => {
            // lock-free mirror: never blocks behind a running turn
            (
                200,
                json!({
                    "service": "pi",
                    "version": env!("CARGO_PKG_VERSION"),
                    "model": info.model,
                    "tools": info.tools,
                    "session_id": info.session_id,
                    "session_dir": info.session_dir.to_string_lossy(),
                    "usage": {
                        "input": info.input.load(std::sync::atomic::Ordering::Relaxed),
                        "output": info.output.load(std::sync::atomic::Ordering::Relaxed),
                        "cost": info.cost.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e8,
                    },
                })
                .to_string(),
                "application/json",
            )
        }
        ("GET", "/sessions") => {
            // session list needs a directory, not the agent (lock-free)
            let rows: Vec<serde_json::Value> = crate::stats::list(session_dir)
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "entries": r.entries,
                        "messages": r.messages,
                        "bytes": r.bytes,
                    })
                })
                .collect();
            (
                200,
                json!({"sessions": rows}).to_string(),
                "application/json",
            )
        }
        ("GET", "/cost") => {
            let (models, (input, output, cost), count) =
                crate::stats::aggregate_cost(session_dir);
            (
                200,
                json!({
                    "model_calls": count,
                    "per_model": models.iter().map(|(m, i, o, c)| json!({
                        "model": m, "input": i, "output": o, "cost": c
                    })).collect::<Vec<_>>(),
                    "total": {"input": input, "output": output, "cost": cost},
                })
                .to_string(),
                "application/json",
            )
        }
        _ => (404, json!({"error": "not found"}).to_string(), "application/json"),
    }
}

/// Optional CORS header injected into every response (empty = disabled).
fn cors_header(cors: &Option<String>) -> String {
    match cors {
        Some(o) => format!("Access-Control-Allow-Origin: {o}\r\n"),
        None => String::new(),
    }
}

/// Write a minimal HTTP/1.1 response (Content-Length framing).
async fn write_resp(
    stream: &mut tokio::net::TcpStream,
    code: u16,
    body: &str,
    cors: &Option<String>,
) -> std::io::Result<()> {
    write_resp_mime(stream, code, body, cors, "application/json").await
}

/// Write a response with an explicit content type.
async fn write_resp_mime(
    stream: &mut tokio::net::TcpStream,
    code: u16,
    body: &str,
    cors: &Option<String>,
    mime: &str,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let body = if body.ends_with('\n') {
        body.to_string()
    } else {
        format!("{body}\n")
    };
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\n{cc}Content-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
        cc = cors_header(cors)
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    Ok(())
}

fn find_head_end(b: &[u8]) -> Option<usize> {
    b.windows(4).position(|w| w == b"\r\n\r\n")
}

fn split_once_crlf(s: &str) -> Option<(&str, &str)> {
    match s.find("\r\n") {
        Some(i) => Some((&s[..i], &s[i + 2..])),
        None => match s.find('\n') {
            Some(i) => Some((&s[..i], &s[i + 1..])),
            None => None,
        },
    }
}

/// Parse a single, well-formed Content-Length header. Returns `None` when
/// absent; rejects duplicates and absurd values (> the 10 MB read guard).
fn content_length_of(head: &[u8]) -> Option<usize> {
    let t = String::from_utf8_lossy(head);
    let mut value: Option<usize> = None;
    for line in t.lines() {
        if let Some(v) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            if value.is_some() {
                // duplicated Content-Length (request smuggling vector)
                return None;
            }
            if v > 10_000_000 {
                return None;
            }
            value = Some(v);
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_contract() {
        let html = DASHBOARD_HTML;
        // endpoints the page relies on
        assert!(html.contains("'/info'"), "info endpoint");
        assert!(html.contains("'/metrics'"), "metrics endpoint");
        assert!(html.contains("'/rpc'"), "rpc endpoint");
        // streaming: reads the response body with a reader loop
        assert!(html.contains("getReader()"), "fetch streaming");
        // user-visible components
        assert!(html.contains("id=\"log\""), "log pane");
        assert!(html.contains("id=\"prompt\""), "prompt input");
        assert!(html.contains("<!doctype html>"), "doctype");
        // remote stop control plugs into the external abort rpc arm
        assert!(html.contains("onclick=\"stop()\""), "stop button");
        assert!(html.contains("type:\'abort\'"), "stop wires abort");
        assert!(html.contains("getReader()"), "fetch streaming keeps working");
    }
}
