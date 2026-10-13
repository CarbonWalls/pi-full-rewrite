# pi (Rust) vs the reference pi

This rewrite targets the same job as the TypeScript reference
(`earendil-works/pi`, `dist/cli.js`) — a terminal coding agent — while
pushing beyond it: same or better features, far less resource use, and a
single static binary.

## Feature parity (the reference's features, implemented here)

| Reference feature | Status | Notes |
|---|---|---|
| Interactive TUI (chat + commands) | ✅ | `/help /model /compact /cost /tools /session /sessions /git /label /theme /find /new /fork /replay /clear /exit`, scrollback (PgUp/PgDn), live theme switching, in-TUI transcript search |
| One-shot prompts (`pi "…"`, stdio) | ✅ | `--mode ask\|code`, `--file`, stdin, `--template`, `--image` |
| Tool calling core | ✅ | 15 built-in tools, parallel dispatch, sandboxed to `--root` |
| Session log (JSONL), resume/continue | ✅ | Per-entry JSONL incl. per-call usage, `--resume <id>` / `--continue`, forks |
| Compaction / context management | ✅ | Auto-compact + `/compact`, `--max-output`, `--tool-timeout` |
| Model routing / fallback / cost | ✅ | 23-model catalog, `custom:` endpoints, `fallback_model`, cost dashboard |
| Todo list tool / plan-first | ✅ | `todo` tool **and** `--plan / -P` |
| `pi --rpc` (stdio JSON-lines) | ✅ | prompt/tools/model/compact/session/export/abort/quit, contract-locked |
| `pi` server mode | ✅ | HTTP: `/healthz /info /sessions /metrics /cost /access-log /dashboard`, streaming `POST /rpc`, `--serve-token`, CORS |
| `/review` against git | ✅ | Compact, bounded for non-repos |
| Skill/template/system config | ✅ | `.pi/pi.json`, `skills/`, `templates/`, `--set`, `$PI_THEME` |
| Auth key management | ✅ | `pi --login provider=key` / `--logout` (`.pi/keys.json`, mode 0600) |

## Beyond the reference

| Extra | What it gives you |
|---|---|
| **`pi --mcp-server`** | pi serves its whole toolset as a Model Context Protocol server (IDE/other agents can drive it) |
| **MCP client + extensions** | `--mcp "server…"` (stdio, 2024-11-05) and `--ext` subprocess protocols |
| **Browser dashboard** | Stream turns live, currently-running prompt with a **■ stop button** (external abort, lock-free), sessions + cumulative cost + metrics panels |
| **External turn abort** | `{"type":"abort"}` over `/rpc` halts the running prompt without blocking on the agent lock |
| **Scenario-driven offline testing** | `PI_MOCK_SCENARIO` replays deterministic multi-step agent runs (incl. `delay_ms` to emulate slow providers) without any API key |
| **Fake-provider E2E tests** | OpenAI-compat, Anthropic, and Gemini dialects (incl. retries/fallback/parallel/abort/compaction) all exercised offline |
| **Per-turn usage + `--cost`** | Sessions record every model call's tokens/cost; `pi --cost` (and `/cost`) aggregate across them |
| **Pi packages** | `scripts/package.sh` bundles `.pi/` config; `pi --install-package` verifies every path (zip-slip-safe) |
| **Static musl build** | One self-contained `aarch64-unknown-linux-musl` binary (~4.4 MB, `ldd` free), scratch Docker image |
| **Shell completions** | `pi --completion bash\|zsh\|fish`, and `--man` |
| **Session tooling** | `--tree`, `--sessions`, `--gc-sessions`, `--export md/txt/html/json`, `--export-all`, `--grep-sessions`, `--stats`, *all* with `--json` |
| **HTTP auth + observability** | `--serve-token` bearer auth, CORS, status-coded `/access-log`, Prometheus-style `/metrics` |
| **`pi --info`** | Build banner with model/tool counts and target |
| **Release sign-off** | `scripts/verify.sh`: 28-check matrix incl. a DEC screen decoder that asserts live TUI content |
| **Four independent audits, all findings closed** | serve/session, tool layer, provider layer, and TUI/CLI reviewed by fresh eyes: framing, CORS, connection caps, path/symlink confinement, retry semantics, SSE caps, control-char sanitization, bounded transcripts, wide chars, honest exit codes |
| **Continuous hardening since** | MCP client bounded reads (1 MiB) + output cap · scenario `error` rules for offline fallback tests · package-install rejects symlink/hardlink/special tar entries (tar-slip) · installer is quiet by default (no profile edits) · 133 tests, 30-point sign-off, CI green |

## Resource use (host: aarch64 release builds)

| | pi (Rust) | reference (Node) |
|---|---|---|
| `--version` | ~13 ms | ~13 s |
| `--help` | ~14 ms | ~28 s |
| idle serve RSS | ~1.9 MB | ~43 MB |
| binary | 4.6 MB release / 4.4 MB static | node_modules + dist |

See [benchmarks.md](benchmarks.md) for the measurement method and full table.
