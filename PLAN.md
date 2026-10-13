# pi-rs — pi rewritten from scratch in Rust

Goal: recreate [earendil-works/pi](https://github.com/earendil-works/pi) (AI agent toolkit:
unified LLM API, agent loop, TUI, coding agent CLI) **from scratch in Rust**, then exceed it:
more features, faster, more efficient. Everything lives in `/root/workspace/research`.

Reference: `../ref/pi-mono` (git clone, read-only).

## Principles
- Single static binary, tiny RSS, ~instant startup (vs node/TS original).
- Own code: hand-rolled TUI diff renderer, hand-rolled SSE, minimal deps.
- Extensions as **subprocesses speaking JSON-lines protocol** → any language (JS, Python, Rust, Go…),
  i.e. superset of pi's TS-only extensions + MCP client.
- All state under `$PI_HOME` (default `~/.pi`), overridable for sandboxed testing.

## Crates (workspace `pi-rs/`)
| crate | replaces | notes |
|---|---|---|
| `pi-ai` | `@earendil-works/pi-ai` | unified LLM API: providers (OpenAI-compat, Anthropic, Google), model catalog, streaming events, auth, retries |
| `pi-agent` | `@earendil-works/pi-agent-core` | agent loop, tool registry/parallel exec, session (JSONL, tree), context compaction, events |
| `pi-tui` | `@earendil-works/pi-tui` | diff-rendering terminal UI: driver, widgets (editor, list, markdown, spinner, images) |
| `pi-tools` | built-in coding-agent tools | read/write/edit/bash/glob/grep + todos, file cache |
| `pi-coding-agent` | `@earendil-works/pi-coding-agent` | binary `pi`: interactive TUI, print/json/rpc modes, slash cmds, extensions, skills, templates, themes, sessions |

Plus `pi-mcp`-ish: MCP client inside `pi-coding-agent` (stdio JSON-RPC).

## Roadmap (milestones, many rounds each)
- [x] M0 scaffold: workspace, deps compile, PLAN
- [x] 1 pi-ai: types + OpenAI-compat + Anthropic + Gemini providers, SSE, model catalog, tests (mock http server)
- [x] 2 pi-agent: loop, tools, sessions, compaction, event stream, tests
- [x] 3 pi-tui: driver + diff renderer + editor + list/markdown; headless render tests
- [x] 4 pi binary: print mode end-to-end (real provider or mock), tools wired, system prompt
- [x] 5 interactive TUI REPL: slash commands (/model /compact /new /clear /theme /login /help /extension /skill)
- [x] 6 extensions: subprocess JSON-lines protocol + loader + examples (JS + Python + Rust)
- [x] 7 MCP client; skills (SKILL.md dirs); prompt templates; themes; session resume/fork/replay
- [x] 8 parallel tools, background bash jobs, todo tool, diff view for edits, image output (kitty)
- [x] 9 hardening: fuzz tests, clippy, packaging (musl static), docs (man + in-app help), benchmarks
- [x] 10 "more": model routing (auto fallback), cost dashboard, /review + git-diff integration,
  `pi serve` (HTTP API), `pi replay`, session search, prompt-caching optimizations

## Status log

All milestones M0–M10 are **complete**. PROGRESS.md records 63 rounds of
work; the codebase now exceeds the reference (see docs/comparison.md and
docs/benchmarks.md): 103 automated tests, a browser dashboard, MCP server
+ client, packages, and a self-verifying release process.
