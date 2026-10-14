# pi-rs

[![CI](https://github.com/CarbonWalls/pi-full-rewrite/actions/workflows/ci.yml/badge.svg)](https://github.com/CarbonWalls/pi-full-rewrite/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/CarbonWalls/pi-full-rewrite)](https://github.com/CarbonWalls/pi-full-rewrite/releases)

A from-scratch Rust rewrite of the **pi** coding agent
(`earendil-works/pi`, TypeScript) — faster, smaller, and with more features.
No code was reused from the reference; everything here is original Rust.

## Design goals

- **Fast.** Single static ~4MB binary. `--version` in ~20ms vs ~12s for the
  reference CLI; full offline mock agent turn in ~28ms; idle serve-mode RSS
  ~600KB vs ~43MB for the Node process. (Startup benchmark against the built
  reference: 250–600× faster; `--help` ~1900× faster.)
- **Static**: `cargo build --release --target aarch64-unknown-linux-musl`
  produces a fully static binary (ldd: "not a dynamic executable") that runs
  on any Linux without dependencies. Uses `musl-clang.sh` (native clang +
  in-dir musl sysroot) as the cross linker.
- **Extensible.** Subprocess extensions, MCP servers, skills, themes, a JSON
  project config, and an HTTP RPC server all speak tiny, documented protocols.
- **Safe by default.** File tools are sandboxed to the working directory;
  `git`/bg-bash/apply-diff are read-only/validated; corrupt session files
  can't panic or hang the agent (fuzz-tested).

## Build

```sh
./build.sh build --release -p pi-coding-agent   # → target/release/pi
./build.sh test                                 # 136 tests, clippy-clean
./scripts/verify.sh                            # 32-point release sign-off
```

Everything (toolchain, caches, targets) stays inside this directory; `build.sh`
pins `RUSTUP_HOME`/`CARGO_HOME` so nothing is installed system-wide.

## Install (published release)

**From the latest GitHub release (static binary, no runtime deps):**

```bash
curl -fsSL https://github.com/CarbonWalls/pi-full-rewrite/raw/master/scripts/install-remote.sh | bash
```

Local builds: `./scripts/install.sh` after `./scripts/release.sh`.

## Quick start

```sh
# interactive
pi
pi -i "first message"                      # open the TUI, send the prompt first
# one shot (stdio)
echo "explain this repo" | pi
pi --json "refactor src/foo.rs and run the tests"
pi "add tests" --model mock/echo --root .   # flags may follow the prompt
pi --mode ask "what is a monad?"          # no tools, conversational
pi --review                              # git status+diff -> review by the model
# automation / IDE
pi --rpc                                  # JSON-lines RPC on stdio (provider mode)
pi --mcp-server                           # serve pi's tools as an MCP server
pi --serve --port 8787                    # HTTP: GET /healthz /info /sessions, POST /rpc
```

### Models

`pi -m <provider>/<model>`. Built-in catalog: `mock/echo` (offline),
`openai/*`, `anthropic/*`, `gemini/*`; `custom:<base_url>:<id>` for any
OpenAI-compatible endpoint; `--fallback <ref>` auto-switches on transport
errors. `pi --list-models` shows the full catalog. API keys come from the
usual env vars or `--api-key`.

## Features vs the reference

| Area | pi-rs |
|---|---|
| Agent loop | streaming, parallel tool dispatch, auto-compaction |
| Sessions | JSONL tree with parent pointers; `--resume <id>`, `--continue`, `/fork`, `/replay`, `--export`, `--stats` |
| TUI | differential cell rendering, markdown, themes (`--theme`/`PI_THEME`), Ctrl-C aborts a turn, piped input works |
| Modes | `code`/`ask` |
| Tools | read, write, edit, glob, grep, list_dir, bash(+timeout), todo, **git (read-only)**, **apply_diff (unified patches)**, **fs** (mv/cp/rm/mkdir), **bg_bash/bg_read/bg_kill/bg_list** (background jobs) |
| Integrations | **MCP client** (`--mcp`), **subprocess extensions** (`--ext`, any language), **image inputs** (`--image` or `@file.png`), **skills** (`--skills`), **project config** (`.pi/pi.json`, `pi --init`) |
| Ops | `pi doctor`, `pi --stats`, `pi --export`, cost dashboard (`/cost`), per-model usage tracking, `pi serve` HTTP |

## Usage

See [docs/usage.md](docs/usage.md) for a complete command tour (every example works with the
mock provider — no API key needed).

## Protocols

- **JSON-lines RPC** (`--rpc`): `ready` → per-request `prompt` / `tools` /
  `model` / `compact` / `usage` / `session` / `echo` / `quit`, with events
  streamed as `{"type":"event","event":…}` and a final `done`.
- **Extensions** (`--ext "<name> <cmd>"`): JSON-lines over stdio.
  Host sends `hello`; extension replies `ready` with `tools` + optional
  `system` text; host calls `call` and broadcasts `event`; extension may send
  `system_append`. Tools surface as `ext:<name>_<tool>`.
- **MCP** (`--mcp "<name> <cmd>"`): JSON-RPC 2.0 over stdio (`initialize`,
  `tools/list`, `tools/call`); tools surface as `mcp:<name>_<tool>`.
- **HTTP serve** (`--serve`): `GET /healthz`, `GET /info`, `GET /sessions`,
  `POST /rpc` (newline-delimited JSON requests → responses).
- **MCP server** (`--mcp-server`): pi exposes its whole toolset as an MCP
  server over stdio — any MCP client (IDE, Claude Desktop, agents) can call
  `read_file`/`bash`/`write_file`/`apply_diff`/… directly. Combine with
  `--mcp`/`--ext` to re-export other servers' tools too.

## Layout

|crate|role|
|---|---|
|`pi-ai`|types, providers (openai-compat/anthropic/gemini/mock), model catalog, retry+fallback, keys |
|`pi-agent`|agent loop, sessions, parallel tool exec, compaction, events, abort |
|`pi-tools`|sandboxed built-in tools |
|`pi-tui`|terminal framework (differential ANSI, editor, markdown, themes, spinner) |
|`pi-mcp`|MCP stdio client |
|`pi-coding-agent`|the `pi` CLI (modes, rpc/serve, extensions, skills, doctor, stats, export) |

## Offline scenario testing

`PI_MOCK_SCENARIO=file.jsonl` makes the mock provider deterministic and
multi-step (no network, no API key). Rules match the last user message
(`contains`) or the last tool result (`after_tool_contains`) and reply with
either a tool call or a plain answer:

```json
{"contains":"create a file","tool":"write_file","args":{"path":"a.txt","content":"hi"}}
{"after_tool_contains":"created","tool":"read_file","args":{"path":"a.txt"}}
{"after_tool_contains":"hi","text":"verified the file"}
```

`echo "create a file" | PI_MOCK_SCENARIO=… pi` drives a full write → read →
verify agent loop — used by the integration tests. A rule may instead emit a
deliberate provider error (`{"error":"primary down"}`), consumed once per run
so `--fallback` can be exercised fully offline:
`{"contains":"x","error":"primary down"}` + `pi -m mock/echo --fallback …`.

## Dependencies (self-contained by design)

Only mainstream, tried-and-tested crates — 13 direct:
`tokio`, `clap` (+`clap_complete`, `clap_mangen`), `serde`/`serde_json`,
`ureq` (HTTP client), `regex`, `sha2`, `hex`, `walkdir`, `glob`, `anyhow`,
`unicode-width`. No unsafe-leaning or exotic crates; the HTTP server, SSE
parser, TUI diff renderer, markdown, and JSONL session engine are all
hand-rolled. 171 packages total in Cargo.lock.

## Testing & hardening

- 136 tests across 6 crates, clippy-clean, `panic=abort` release profile.
- **Provider E2E, offline:** fake local API servers drive the REAL
  OpenAI-compat, Anthropic and Gemini code paths — request shaping, SSE
  parsing, tool-call deltas, real tool execution, tool-result round trips,
  retries (500→503→200), fallback, parallel dispatch, abort and compaction
  (`crates/pi-agent/tests/fake_*.rs`, `retry.rs`, `multi_tool.rs`,
  `fallback.rs`, `compact.rs`, `abort.rs`).
- **Binary-level:** the real `pi` binary is spawned for HTTP serving
  (`http_serve.rs`), stdio RPC (`rpc_stdio.rs`), session resume
  (`resume.rs`) and `--plan` (`plan.rs`) via `CARGO_BIN_EXE_pi`.
- Fuzz-style tests feed corrupt JSONL (random bytes, truncated lines,
  parent cycles) into session loading; `branch()` is cycle-proof; a
  full session round-trip test covers roles, forks, tool messages and
  compactions.
- The interactive TUI is exercised under a real PTY (`script(1)`): message
  turns, slash commands, Ctrl-C abort/quit, piped input, terminal restore.

## Known limits

- Real *commercial* provider accounts are not reachable from this sandbox;
  the offline fake-server tests cover the same wire protocols
  (OpenAI-compat, Anthropic, Gemini) without keys.
- The TUI transcript is capped to the viewport (+ PgUp/PgDn scrollback); it
  is not a full pager for arbitrarily long sessions.
