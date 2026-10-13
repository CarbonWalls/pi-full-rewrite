# Changelog

All notable work on pi-rs (the Rust rewrite of pi).

## 0.1.0 — current

### Remote control
- `pi serve` + browser dashboard: stream turns live over `POST /rpc`,
  live metrics, and a **■ stop button** that aborts the running turn
  (`{"type":"abort"}` — handled lock-free so it never blocks).
- RPC protocol: prompt/tools/model/compact/session/export/abort/quit.

### Agent core
- Full agent loop: streaming events, parallel tool dispatch, tool-result
  round trips, hard turn caps.
- Sessions as append-only JSONL with forks (`/fork`), labels (`/label`),
  compaction (auto + `/compact`), resume (`--continue`/`--resume`),
  persistence across server restarts.
- Cooperative Ctrl-C/SIGTERM abort; graceful TUI + serve shutdown with
  terminal restore; panic-audited code paths.

### Providers
- OpenAI-compat (incl. `custom:` endpoints, `[DONE]` handling, reasoners),
  Anthropic Messages, Gemini generateContent (alt=sse), DeepSeek, Mistral,
  OpenRouter, Groq; automatic `--fallback`; retry/backoff with `Retry-After`;
  deterministic mock + `PI_MOCK_SCENARIO` offline driver.
- Cost dashboard + usage tracking; `--max-output`, `--tool-timeout`.

### Interfaces
- Interactive TUI: markdown rendering, editor, slash commands, PgUp/PgDn
  scrollback, Tab completion (commands + model refs), status line, themes,
  `/theme` live switching, in-TUI `/find` transcript search.
- stdio JSON-lines RPC (`--rpc`), HTTP serve (`/healthz /info /sessions
  /metrics /access-log`, streaming `POST /rpc`, `--serve-token`, `--cors`),
  MCP server (`--mcp-server`), MCP client (`--mcp`), subprocess extensions
  (`--ext`).
- Print/JSON modes, `--file`, `--template`/`--set`, `--image`, `--plan`,
  `--review`, `--export` (md/txt/html/json), `--export-all`, `--grep-sessions`,
  `--install-package`, `--login`/`--logout`, `--tree`, `--stats --json`,
  `--sessions --json`, `--doctor --json`, `--info [--json]`,
  shell completions, `--man`.

### Operations
- `scripts/release.sh` bundles both binaries + docs + scripts;
  `scripts/verify.sh` runs a 24-point release sign-off (curl-free) that
  now decodes the live TUI frame to assert transcript content and
  `/find` results.
- Static aarch64 musl build (single ELF, no dynamic deps).

### Reliability
- 111 automated tests: provider E2E against fake servers, binary-level
  protocol tests, HTTP fuzz + strict framing, apply_diff property tests,
  session round trips, TUI PTY runs, plan-driven agentic tasks,
  tool-error recovery, serve concurrency, concurrency + external abort.
- Found-and-fixed live bugs: `[DONE]` SSE terminator, `custom:` URL parsing,
  session fork parent pointers, TUI trailing-var-arg prompt swallowing,
  HTTP double-CRLF framing (8 rounds masked by lenient curl), TUI dropping
  an instantly-completed turn's transcript (reclaim ran before drain),
  serve-side turns being un-abortable, binary-size and startup
  optimizations (~900× faster than the Node reference).
