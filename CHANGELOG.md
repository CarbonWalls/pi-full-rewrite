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

## 1.0.7 — HTTP /access-log framing fix
- With `--cors` unset, `/access-log` ended its HTTP headers right after the
  status line (a stray `\r\n` after the empty CORS header) so the real
  headers leaked into the body. Fixed; the 30-point sign-off now asserts the
  body starts clean.

## 1.0.6 — package-install tar-slip fix
- Package installs now reject any non-regular/non-directory tar entry
  (symlinks, hard links, specials) — writing through a link could previously
  escape `<root>`. Regression-tested; 30-point matrix covers the valid path.

## 1.0.14 — stdio RPC @-image attachment (prompt-path parity)
- `--rpc` prompts now attach in-line `@file.png` tokens exactly like the
  CLI and HTTP `/rpc` paths (a shared `inline_image_parts` helper); the
  explicit `parts` frame field is honoured. All three prompt paths behave
  identically.

## 1.0.13 — HTTP RPC @-image attachment (CLI parity)
- `POST /rpc` prompts without an explicit `parts` field now run the same
  in-line `@image` attachment as the CLI prompt path. The serve runtime
  gained a `ServeOpts` struct (argument-count lint stays zero).

## 1.0.12 — --doctor diagnostics
- `--doctor` now checks git identity (`user.name`/`user.email`, the
  `--review` prerequisite) and runs a real offline mock turn to prove the
  local stack end to end.

## 1.0.11 — theme spec honesty
- A `--theme`/`$PI_THEME` value that is neither inline JSON nor a readable
  theme file now prints a one-time warning instead of silently falling back.

## 1.0.9/1.0.10 — root confinement for file ingress
- `--out`, `--file`, and `--image` are all resolved through the shared
  `resolve_under` check: `..`/absolute paths that escape `--root` are
  rejected with a clear error. The 32-point sign-off locks them.

## 1.0.8 — deterministic session ordering + honest --export
- Session ordering/GC now use sub-second mtime precision (same-second files
  no longer sort arbitrarily). `--export <missing>` fails loudly (rc=1)
  instead of silently exporting an empty session.

## 1.0.5 — MCP client hardening
- `pi-mcp` reads are bounded (1 MiB per line) and tool output is capped at
  1 MiB with a truncation notice — a hostile MCP server can no longer grow
  the agent's memory unboundedly (same class of fix as the provider SSE
  caps). A fake-server regression test locks it. 131 tests; CI green.

## 1.0.2 — process-reaping hardening
- bash timeout / `bg_kill` now reap the descendant tree before killing the
  shell (a shell death reparents children to init and strands them), then the
  process group, then the child — covers job-control sub-groups too.
- The ancestry walk runs before the `pkill -P` net (pkill first would
  reparent deeper descendants out of reach). Two regression tests lock both
  axes. CI green on GitHub Actions; 131 tests.

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
