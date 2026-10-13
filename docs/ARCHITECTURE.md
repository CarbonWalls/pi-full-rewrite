# pi-rs architecture

A Rust workspace (aarch64, rustc stable) implementing a terminal coding agent
— the same job as the TypeScript `pi`, minus the interpreter overhead.

## Crates

```
pi-ai            types + model catalog + HTTP providers + offline mocks
pi-agent         the agent loop: sessions, turns, tools, compaction, abort
pi-tools         the 15 built-in tools (file, shell, git, todo, bg, diff, fs)
pi-tui           terminal primitives: buffer, markdown, editor, theme, spinner
pi-mcp           MCP client (JSON-RPC 2.0, 2024-11-05)
pi-coding-agent  the `pi` binary: CLI, TUI, rpc, serve, mcp-server, ext, export
```

Dependencies point one way: `pi-coding-agent` → everything; `pi-agent` →
`pi-ai`; `pi-tools`/`pi-tui`/`pi-mcp` → `pi-ai` types only. No cycles.

## pi-ai

- `types.rs` — `ChatMessage`, `ModelInfo`, `ChatRequest`, roles, tool specs,
  prices; `Api` parse + `default_base_url`.
- `catalog.rs` — embedded list of ~23 models (openai/anthropic/google/
  deepseek/openrouter/mistral/groq/custom), `OnceLock`.
- `http.rs` — ureq-based `for_each_sse` (SSE framing), `post_with_retry`
  (retryable 5xx backoff, `Retry-After`), `make_agent` (timeouts, TLS).
- `providers/` — `openai.rs` (OpenAI-compat + `[DONE]` + tool deltas +
  fallback), `anthropic.rs` (message events, tool-use JSON deltas), `gemini.rs`
  (alt=sse, functionCall), `mock.rs` (echo + `@tool`), `scenario.rs`
  (`PI_MOCK_SCENARIO` JSONL rules). All stream through
  `StreamEvent` callbacks with cooperative abort.
- `lib.rs` — `Llm` facade: `model(ref)`, `stream(req, on_event)`,
  `Keys::from_env`, `cost_dashboard`.

## pi-agent

- `session.rs` — append-only JSONL log with `Entry { id, parent, kind, body,
  ts_ms }`. Forked branches are just entries whose `parent` isn't the tail.
  `head` is the fork pointer; `push_user_message` branches from it,
  `push_message` chains from the tail. `branch()` walks parents (cycle-proof),
  `label()/push_label()` for `/label`. `flush()` appends one entry per turn.
- `lib.rs` — `Agent` owns `Llm + config + tools + session`.
  - `user_turn_parts(input) -> UnboundedReceiver<AgentEvent>` runs the whole
    turn INLINE and returns a replay receiver once done (never blocks on the
    caller's drain). `start_turn(self, input) -> Turn` spawns its own task
    (used by the TUI); `Turn::{recv, try_recv, finish, take_agent}`.
  - `one_cycle` POSTs the current branch, streams events into an unbounded
    channel while a `spawn_blocking` task drives the provider; a shared
    `Arc<AtomicBool>` abort is checked per event (cooperative).
  - Tool calls fan out through `spawn_blocking` in parallel; tool results
    become `Role::Tool` messages and the loop cycles until no tools remain
    (`max_turns` cap). `finish_reason: length` triggers `auto_compact`
    (summarize + `Compaction` entry); `fallback_model` is retried when the
    primary fails.

## pi-tools

All tools implement `pi_agent::tools::Tool` (spec + run); paths resolve
relative to the sandboxed root. Highlights: `bash` (process group, group-kill
on timeout, per-call `timeout_ms` override), `git` (read-only whitelist),
`todo` (wholesale list replacement), `bg_*` (spawned jobs with stdout
capture), `apply_diff` (unified diff engine, verified equivalence to `git
diff`), `edit_file`, `read/write/glob/grep/list_dir`, `fs`.

## pi-tui

Differential ANSI renderer (`Buffer`, `diff`, `render_to_string`), markdown →
styled lines, editor with history, `Theme` (palette + JSON parse),
`Spinner` with width reporting. Drop-in headless-testable.

## pi-coding-agent (bin `pi`)

- `main.rs` — clap CLI (flags parse before/after the prompt), adaptive tokio
  runtime (multi-thread only when the interactive TUI needs it), mode
  dispatch: prompt/print/json/rpc/serve/mcp-server/export/stats/tree/sessions/
  doctor/info/completions/man. `build_user_parts` handles `@image` tokens.
- `ui.rs` — the TUI loop: single `dispatch_input`/`submit_input` funnel,
  queued-input drain when idle, model/session status line, PgUp/PgDn
  scrollback, Tab completion (slash commands + model refs), SIGTERM/SIGINT
  graceful exit through `Tui::stop()`, Ctrl-C abort.
- `serve.rs` — minimal HTTP/1.1 server: `/healthz /info /sessions /metrics
  /access-log`, streaming `POST /rpc` (NDJSON), bearer auth, CORS, per
  connection 120 s cap, strict framing.
- `rpc.rs` — stdio JSON-lines protocol (shared by serve): tools/model/
  compact/session/prompt/quit.
- `mcp_server.rs` — expose the whole toolset as an MCP server.
- `ext.rs` — subprocess extensions (JSON-lines hello/ready/call/result).
- `stats.rs`, `export.rs`, `review.rs`, `doctor.rs`, `config.rs`, `skills.rs`,
  `templates.rs`, `completion.rs` (shell completions + `--man`).

## Data flow (one print-mode turn)

clap → model resolve → `Agent::new(...)` → `user_turn_parts` → push user →
`one_cycle` → provider SSE → tool calls → parallel `run_tools` → cycle again
→ final text → events drained from the replay receiver → session flushed to
JSONL.

## Offline testing

`tests/fake_openai|anthropic|gemini.rs` run real provider code against fake
local HTTP servers (request shaping, SSE dialects, tool deltas, fallback,
retries, abort, compaction, parallel dispatch). Binary-level tests spawn
the real `pi` via `CARGO_BIN_EXE_pi` (HTTP rpc, stdio rpc, serve auth,
resume, plan, diff, timeout, framing, fuzz). TUI tests run under a real PTY.
`scripts/release.sh` + `scripts/verify.sh` produce and sign off the bundle.

## Releases & CI (GitHub)

The published repo (github.com/CarbonWalls/pi-full-rewrite) self-releases:

1. **CI (every push):** pinned toolchain (1.99.0) → 131 tests, clippy 0,
   32-point sign-off (`scripts/verify.sh`), tree-cleanliness guard — plus a
   separate job that builds and verifies the static musl binary.
2. **Tag-release (every `v*` tag):** runs the same gates, builds glibc
   amd64 + static musl amd64, bundles README/docs/scripts + VERSION, and
   creates a GitHub Release with the binaries.
3. **Install:** `scripts/install-remote.sh` downloads the static binary from
   the latest release and installs it (also invoked via the README one-liner).

Both shipped binaries self-identify: `--info` reports the exact build commit
(`PI_BUILD_SHA`). The arm64 static binary is built on an aarch64 host
(runner's musl-gcc cannot cross-link aarch64) and can be attached to the
release manually. Historical note: the sign-off tests set hermetic git
identity because GitHub runners ship none, and a timeout path reaps the
descendant process tree before killing the shell (see bash.rs).
