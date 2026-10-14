# PROGRESS — pi-rs

Per-round log (newest last). All work in /root/workspace/research.

## Round 1
- Surveyed environment: rustc 1.85 system (too old for icu4x deps) → installed stable
  rustc 1.99.0 via rustup **inside workspace** (`.rustup/`), nothing outside.
- Cloned reference: `ref/pi-mono` = earendil-works/pi (TypeScript, ~390k lines).
- Created workspace `pi-rs/` with 5 crates: pi-ai, pi-agent, pi-tui, pi-tools,
  pi-coding-agent. `build.sh` pins CARGO_HOME/RUSTUP_HOME inside the dir.
- **pi-ai (done, compiling, 5 tests green):**
  - types: Role/ChatMessage/ContentPart/ToolCall/ToolSpec/ChatRequest/StreamEvent/
    Usage/ModelInfo/ThinkingLevel/StopReason/Api families + custom-api overlays.
  - providers: openai-compat (any base_url), anthropic (cache_control + thinking),
    gemini (alt=sse), mock (offline echo + tool simulation).
  - SSE framing, retry w/ backoff + Retry-After, fallback-model chaining,
    per-read stall timeout, zero async deps (bridge via spawn_blocking).
  - model catalog: embedded models.json + local overlay ($PI_HOME/models.json);
    key resolution: keys file → env.
- **pi-agent (done, compiling, 3 tests green):**
  - Agent loop: user_turn → stream LLM → parallel tool exec → persist → repeat.
  - ToolRegistry (Send+Sync tools, spawn_blocking fan-out).
  - Session: JSONL tree (parent pointers), branch replay, compaction entries,
    fork, branch_hash. Fixed an id-collision bug (atomic counter).
  - Auto-compact on length overflow; /compact summarize via one-shot LLM call.
  - AgentEvent stream (user/start/deltas/tool/result/assistant/usage/error).
- Next: pi-tui (diff renderer + editor + markdown), pi-tools (read/write/edit/
  bash/glob/grep), pi-coding-agent CLI (print/json/rpc modes, slash cmds).

## Round 2 (2026-07-16)
- **pi-tui (done, 9 tests green):** cell-level differential renderer
  (CUP+SGR, 24-bit colors packed in u32), buffer/area/widgets, markdown →
  styled runs (headings/bold/italic/code/fences/bullets/links), multi-line
  editor w/ history + paste, braille spinner, crossterm-0.28 driver
  (raw mode, alt screen, poll with timeout, resize-safe).
- **pi-tools (done, 5 tests green):** 8 sandboxed tools — read/write/edit
  (literal replace w/ uniqueness check + replace_all), glob, grep (regex,
  200-line cap, skips dot/target/git/node_modules), list_dir, bash (sh -c,
  timeout, concurrent stdout/stderr drain capped 20k, `[exit N]` footer),
  todo (status counts). `Toolset::resolve` rejects `..` escapes; tests now
  use per-test temp dirs (parallel-same-PID fix).
- **pi-coding-agent CLI (smoke-tested with mock):**
  - clap front: prompt/stdin, -m/--model (incl. `custom:<url>:<model>`),
    --root confinement, --system, --api-key/--base-url, --json, -i,
    --no-tools, --max-turns/--temperature/--thinking, --rpc,
    --list-models (23 builtins), --resume, --quiet, --session-dir.
  - print mode: streamed text on stdout, tool trace on stderr, usage/cost
    footer; `--json` = one event per line; stdin pipe works.
  - rpc mode: JSON-lines protocol (prompt/tools/model/compact/usage/session/
    quit) — verified end-to-end.
  - interactive TUI wired: transcript (markdown) + spinner + editor,
    Turn-ownership flip (agent in flight / idle), queued input, slash
    commands /help /model /compact /cost /tools /session /clear /exit.
  - `Agent::start_turn`/`Turn` + `compact_now` in pi-agent; AgentEvent gets
    explicit JSON shape via manual Serialize (tagged newtype variants
    can't be derived).
- **Hygiene:** clippy-clean across workspace (rustup component added inside
  dir), all 22 tests green. Mock provider remains the offline test path.
- Known: interactive mode not exercised under a real TTY yet; real
  provider paths still untested without keys.

## Round 3 (2026-07-16)
- **pi-mcp (new crate, 2 tests green):** MCP client over JSON-RPC 2.0 /
  stdio subprocess: `initialize` (2024-11-05) → `initialized` → `tools/list`
  → `tools/call`. `McpTool` wraps the client (`mcp:<server>_<tool>` names,
  text/json content, `isError` + `structuredContent` fallback). Synchronous
  API run in `spawn_blocking`; Drop kills the child. `connect_tools()`
  returns `Vec<Arc<dyn Tool>>` for registry integration.
- **Extension protocol v1 (`ext.rs`, 2 tests green):** host↔subprocess
  JSON-lines in any language. hello→ready(tools+system), call→result,
  event broadcast, `system_append` into the system prompt. Tools exposed as
  `ext:<name>_<tool>`; handles kept for event fan-out.
- **Session persistence:** `attach_path` auto-appends entries to
  `<id>.jsonl` (flush on write); `--continue` = newest by mtime;
  `--resume <id>`; `Session::new(None)+attach` = fresh persistent session.
- **git tool + /review (M10):** shell-free `git` tool (whitelist of
  read-only subcommands; blocks `-C -c --git-dir* --work-tree* 
  --namespace*`, allows `--cached`; 100k cap; cwd=root). `--review` CLI /
  `/review` TUI feed status + last-3 commits + unstaged/staged diff into a
  prioritized review prompt.
- **Background bash (M8):** `BgRegistry` + `bg_bash/bg_read/bg_kill/bg_list`
  tools. Detached drain threads → shared capped buffers + spill files
  under `.pi/bg/`; lazy `try_wait` reap; offset-tailed reads.
- **Cost dashboard (M10):** per-model usage + cost maps on `Agent`
  (`model_usage`/`model_cost`); `/cost` renders a markdown table.
- **pi serve (M10):** `--serve [--port]` — HTTP JSON-lines RPC over TCP
  (GET /healthz /info, POST /rpc). Agent serialized behind a tokio mutex
  (one prompt at a time, matching `--rpc`). curl-verified end-to-end.
- **Hygiene:** 28 tests green (pi-ai 5, pi-agent 3, pi-tui 9, pi-tools 7,
  pi-mcp 2, pi-coding-agent 2); clippy 0 warnings workspace-wide; release
  binary ~4.2MB, cold start ~16-27ms, full mock turn ~20-28ms.
- **Reference benchmark (M9):** `ref/pi-mono` npm-installed and built
  (npm workspaces; TS7 Go tsc works under npm's correct layout). **pi-rs is
  dramatically faster**: `--version` 24-51ms vs ref pi 11.6-14.4s
  (~250-450x); `--help` 22ms vs 42.8s (~1900x). Idle serve-mode RSS: 612KB
  vs ~43MB for the Node process at startup. Binary: single static 4.3MB
  executable vs 8.7MiB JS bundle + node runtime. (Ref has no offline mock,
  so turn-by-turn comparison isn't possible; startup+RSS is the control.)
- **Themes (M6):** `pi-tui::Theme` (named palette: h1/h2/bold/code/link/dim/
  body/user/assistant/tool/error/spinner/prompt). Load from `--theme <json|path>`
  or `$PI_THEME`; accepts `#rrggbb`, `rgb(r,g,b)`, `[r,g,b]`, `"default"`.
  Markdown renderer is theme-aware (`render_with`/`draw_markdown_with`).
- **Model fallback (M10):** `--fallback <ref>`; AgentConfig.fallback_model
  flows into ChatRequest (pi-ai's transport-error fallback chain).
- **Hygiene:** 30 tests green (pi-tui +2 theme tests); clippy 0 warnings
  workspace-wide.

## Round 4 (2026-07-16)
- **fs tool (M8):** sandboxed move/copy/remove/mkdir (recursive dir remove
  requires explicit `recursive:true`; escapes blocked). Fills the gap where
  agents previously couldn't create dirs or delete/move files.
- **Skills (M6):** `--skills name1,name2|all` + `--skills-dir` (default
  `<root>/.pi/skills/*.md`, front matter name/description; body appended to
  the system prompt; unknown names reported).
- **pi doctor (M9):** environment diagnostics: git/sh/python3/node probes,
  working dir, git-repo check, disk space (df -k), TTY, provider keys.
- **pi stats (M10):** `--stats <session.jsonl>` — entries, branch depth,
  message/role/tool_call/char counts from a recorded session.
- **Cooperative abort:** `Agent.abort` (Arc<AtomicBool>) checked in the
  stream callback; TUI Ctrl-C aborts an in-flight turn (Ctrl-C again quits),
  and the flag is cleared on turn reclaim.
- **Fuzz/robustness (M9):** corrupt-JSONL + Xorshift pseudo-fuzz session
  tests; **fixed a real hang**: `Session::branch()` could infinite-loop on
  cyclic parent pointers (now guarded by seen-index + step limits).
- **apply_diff tool (M8):** unified-diff application matching the reference
  pi's `apply_patch` — parse `@@ -S,C +N,M @@` hunks, apply bottom-up with
  strict context verification (no partial writes), insertion hunks for new
  files, sandboxed to root. Verified with real `git diff` output. 3 unit +
  1 integration test.
- **Image inputs (M8):** `--image <file>` (repeatable) + `@file.png` tokens
  inside the prompt auto-attach as `ContentPart::Image` (base64 data URI,
  correct media type) — matching the original's image-in-prompt behavior.
  New `Agent::user_turn_parts` accepts arbitrary content parts.
- **Project config + `pi init` (M6):** `<root>/.pi/pi.json` provides
  defaults (model/system/temp/max_turns/thinking/skills/theme/session_dir/
  compact_threshold); CLI flags override the config (verified: bad config
  model errors, `-m` still wins). `pi --init` scaffolds `.pi/pi.json` +
  skills/ + sessions/. `-m` is now optional (default falls back to config
  then `mock/echo`).
- **Hygiene:** 41 tests green, clippy 0 warnings, dev+release build clean.
- **TUI validated under a real PTY (script(1)), M5 finally [x]:** drove pi -i
  with piped input — a message turn ran (User → mock reply → persisted to
  session) and `/exit` exited cleanly. Found + fixed **three real bugs** in
  the process:
  1. `Frame::render` subtracted with overflow on small terminals (now
     saturating) — panicked under a tiny PTY.
  2. crossterm reports `\n` in raw mode as **Ctrl+J** (`Char('j')`+CONTROL);
     piped input never produced Enter. TUI now maps Ctrl+J → Enter so
     `echo x | pi -i` works; multi-line paste submits line-by-line
     (`dispatch_input` shared by Enter/paste/queued paths).
  3. `Turn::finish()` discarded the `Some(agent)` from its loop and called
     `take_agent()` again → panic on every interactive turn reclaim. Now
     returns the agent from the loop directly (+ regression test).
- **pi export (M10):** `--export <session.jsonl> [--format md]` renders any
  recorded session as a readable markdown transcript (user/assistant/tool
  messages with tool-call summary lines).

## Round 5 (2026-07-16)
- **Modes (original pi feature):** `--mode code|ask`; ask mode disables all
  tools and swaps the system prompt to a conversational persona (verified:
  `@tool` triggers 0 calls in ask vs 1 in code). Config-file projects default
  to their stale `mock/echo` only when no `-m`.
- **Ctrl-C fixed (PTY test):** crossterm reports a real Ctrl-C (0x03) as
  `Char('c')`+CONTROL — the old `Char('\u{3}')` check never matched. Now
  matched via modifier bits; idle Ctrl-C exits 0, in-turn Ctrl-C aborts.
- **README.md** with quick start, feature table, protocols, layout, testing.
- **Final benchmark (release):** `--version` ~30ms, `--help` ~49ms, full mock
  turn ~52ms; binary 4.4MB. (Reference pi: `--version` 11.6–14.4s.)
- **Bare `pi` now opens the interactive TUI** like the original: removed
  `arg_required_else_help` (which made no-arg invocation print help; only
  `--help`/flags count as args). PTY-verified: `pi` → renders, runs the
  message turn, `/exit` clean.
- **`--file <path>` / `--out <path>`:** read the prompt from a file (resolved
  under `--root`), and write the final answer to a file — handy for scripts
  and IDE/MCP-style callers.
- **Hygiene:** 44 tests green, clippy 0, build clean in both profiles.

## Round 6 (2026-07-16)
- **Fully-static musl build — M9 ✅:** downloaded the musl cross sysroot
  *inside the workspace* (no system installs). The musl.cc gcc binaries
  SIGBUS on this host, so the **native clang** drives the link
  (`musl-clang.sh --target=aarch64-linux-musl --sysroot=<in-dir> -static`).
  `target/aarch64-unknown-linux-musl/release/pi` = **4.24MB, ldd: "not a
  dynamic executable"**, runs anywhere, `--version` 18ms, full mock turn OK.
- **current_thread tokio runtime → faster startup:** `--version` 20-27ms and
  the **full mock agent turn dropped ~46%: 52ms → 28ms** (agent work already
  runs on spawn_blocking, so throughput is unchanged).
- **`--sessions` + `--gc-sessions N`:** list recorded sessions (id, entries,
  msgs, size, local timestamp; newest first) and prune old ones. TUI gained
  `/sessions`.
- **Prompt templates (M6 gap ✅):** `.pi/templates/<name>.md` with
  `{{var}}` interpolation; built-ins `root/cwd/os/model/date/time` plus
  `--set k=v`; `--template <name>` beats stdin/prompt. E2E verified.
- **Hygiene:** 47 tests green, clippy 0.

## Round 7 (2026-07-16)
- **Adaptive tokio runtime — fixed a real TUI deadlock:** the TUI's
  crossterm `poll()` is a *blocking* fd wait; on a single-threaded runtime it
  starved the spawned turn task (messages never ran, no session written).
  `main()` now selects a **multi-thread runtime for the TUI** and a
  `current_thread` runtime for everything else (print/rpc/serve stay fast).
  PTY-verified: turns run + persist, `/exit` clean, 0 panics.
- **Queued-command drain fix:** slash commands queued while a turn was busy
  (e.g. `/model` then `/exit` during a long turn) once got stuck — the old
  code only drained the queue on turn completion, so a queued non-message
  command stranded everything after it. Now drained whenever idle, per item.
  PTY-verified with `/hello /model /compact /exit` → clean exit.
- **HTML session export:** `--export <session> --format html` yields a
  self-contained styled transcript (escaped, role-colored). `cargo doc`
  warning-free.
- **`--no-session`:** fully stateless runs (nothing written to disk).
- **`pi serve` polish:** `--serve-addr <ip>` binding, `GET /sessions`
  (JSON session list), graceful shutdown on SIGINT.
- **bash tool hardening:** commands now run in their own process group and
  timeouts kill the whole group — no orphaned grandchildren (tested: leader
  + child `/proc` pids both gone after a 400ms timeout).
- **Examples:** `examples/` with sample `.pi/pi.json`, skills, a template,
  and a JSON theme.
- **Full-loop integration test:** real `Toolset` + mock through the entire
  agent loop (tool call → result → final → persisted session).

## Round 8 (2026-07-16)
- **Scenario-driven mock provider (offline agentic tests):** `PI_MOCK_SCENARIO`
  JSONL rules — `contains` (last user msg) / `after_tool_contains` (last tool
  result) → `tool"+args` or `text` — drive deterministic multi-step loops
  with **no network**. Verified E2E: write_file → read_file → confirm
  (3 tool/answer steps persisted in the session) + pi-agent regression test.
  `docs`: README "Offline scenario testing".
## Round 9 (2026-07-16)
- **Fixed a real production deadlock: large outputs hung the agent.** `user_turn`
  ran the turn inline and only handed back the receiver *after* the turn
  finished, but events were forwarded through a bounded (512) channel — so any
  output larger than 512 events deadlocked (the unconsumed channel blocked the
  turn; the turn blocked `user_turn` from returning so the caller could drain).
  The 15KB `/review` prompt and `--file`-fed large prompts exposed it (also via
  the TUI). Fix: `user_turn`/`user_turn_parts` now deliver through an
  **unbounded** channel (backlog bounded by model output); `one_cycle`,
  `run_tools`, `compact` use `UnboundedSender`. Large-prompt print run: 4000
  words complete in ~100ms with full persistence (regression tests:
  `pi-agent/tests/big.rs`, plus the pre-existing scenario tests).
- **Test-flake fix:** two session fuzz tests shared one temp dir name and
  deleted each other's files under parallel execution; dirs are now distinct.
## Round 10 (2026-07-16)
- **`--mcp-server`: pi itself as an MCP server (beyond the original).** Exposes
  pi's entire toolset (built-ins + any `--mcp`/`--ext` tools) to ANY MCP
  client over stdio JSON-RPC: `initialize`, `notifications/initialized`,
  `tools/list` (full input schemas), `tools/call`, `ping`, `resources/prompts
  list`, proper -32700/-32601/-32602 errors. Verified: (1) Python MCP client
  drives mkdir/write/read through the server; (2) **dogfood** — pi's own MCP
  client (`--mcp "self pi --mcp-server"`) connects to a nested pi server and
  runs tools end-to-end. Unit test in `mcp_server.rs`.
- **Hygiene:** 55 tests green, clippy 0.

## Round 11 (2026-07-16)
- **Streaming `pi serve` /rpc:** responses now stream as `application/x-ndjson`
  (header first, one flushed line per event/result) instead of buffering until
  the turn ends — long LLM runs are visible in real time (verify: full mock
  turn arrived in ~7ms across 9 event lines).
- **`--tool-timeout <ms>`:** global default for the bash tool (Toolset gained
  `with_tool_timeout`; default still 60s).
- **`--max-output <n>`:** clamp per-turn max output tokens on the active model.
- **Hygiene:** 55 tests, clippy 0.

## Round 12 (2026-07-16)
- **`pi --mcp-server` (beyond the original):** pi serves its *entire* toolset
  (built-ins + `--mcp`/`--ext` tools recursively) as an MCP server over stdio
  JSON-RPC: `initialize`/`tools/list` (full input schemas)/`tools/call`/
  `ping`/resource+prompt lists, proper JSON-RPC errors. Verified two ways:
  (1) an external Python MCP client drives write/read/bash through the server;
  (2) **dogfood** — pi's own MCP client points at pi's server
  (`--mcp "self pi --mcp-server"`) and runs tools end-to-end.
- **Streaming `pi serve` /rpc:** events flush as `application/x-ndjson` lines
  in real time (no buffering until turn end); full tool turn observed in ~7ms.
- **`--tool-timeout <ms>`** (global bash default; `Toolset::with_tool_timeout`),
  **`--max-output <n>`** (per-turn output cap), **`--tree <session.jsonl>`**
  (fork-aware entry tree with ▶ head markers), **`--stats --json`** (machine
  readable) added. TUI tool-flow validated under a real PTY with the scenario
  mock (write_file actually created the file).
- **Hygiene:** 56 tests, clippy 0. Release binary re-built.

## Round 13 (2026-07-16)
- **apply_diff property test:** 150 deterministic pseudo-random files/mutations
  — each file is committed, mutated, `git diff` generates the authoritative
  patch, and pi's apply_diff must reproduce the mutation *exactly* (round-trip
  through registry.execute_named). 49s/250 seeds initially, trimmed to 150.
- **`/git [args]` TUI command:** read-only git (same whitelist as the tool)
  with output rendered in the transcript; PTY-validated.
- **`--serve-token <secret>`:** bearer auth for every `/serve` route
  (`healthz` stays open for probes); 401 on missing/wrong token — verified
  across /info and /rpc.
- **`/info` enrichment:** version, session id added to the serve info endpoint.
- **Hygiene:** 57 tests, clippy 0.

## Round 14 (2026-07-16)
- **Fixed session forking (real branch-management bug):** `push_message` used
  `entries.last()` as the parent, so after `/fork <n>` the next user message
  appended at the end of history instead of branching from the fork point.
  Split into `push_user_message` (branches from the head tip) vs `push_message`
  (assistant/tool replies chain from their own user more), with `head`
  recomputed as `entries.len()` after each append. PTY-verified end-to-end:
  `/fork 1` then a new message yields a perfect two-way tree
  (`user alpha →▶ user beta → assistant echo:beta`, old `echo: alpha` shown as
  an off-mainline fork), with persistence across reloads + regression test
  (`fork_rewinds_parent_pointer`).
- **Hygiene:** 58 tests, clippy 0.

## Round 15 (2026-07-16)
- **`/review` prompt hardening:** git prints its entire usage help when run
  outside a repo, which once ballooned the review prompt to 15KB. Non-repo
  stderr is now collapsed to its first line (+cap), and each section is capped
  at 20K chars; a non-repo review prompt is verified < 5K chars.
- **Static musl rebuild with all features:** the aarch64-unknown-linux-musl
  binary was stale; rebuilt (4.35MB) and verified it serves `--mcp-server`
  handshakes, stats/export/doctor across the whole stack. (Only warning is
  clang's benign `-no-pie` linker notice.)
- **CI workflow:** `.github/workflows/ci.yml` (tests + clippy + release +
  static build on ubuntu runners).
- **Hygiene:** 59 tests, clippy 0.

## Round 16 (2026-07-16)
- **Fixed the trailing-var-arg CLI wart (significant):** `trailing_var_arg`
  on the prompt swallowed *every flag after the first positional* — e.g.
  `pi -i "prompt" --root X --model y` turned the flags INTO the prompt text
  (breaking session-dir, root, model). Dropped it: flags parse anywhere,
  multi-word prompts still join, `--` protects dash-prefixed prompts.
  Regression test (`flags_may_follow_the_prompt`).
- **`pi -i "prompt"` now opens the TUI and sends the prompt as the first
  message** (was: silently ran print mode). Added `submit_input` so editor/
  paste submissions preserve queue ordering (a fast `/exit` can't jump ahead
  of an already-queued prompt). PTY-verified: initial prompt then typed
  message then `/exit`, both persisted in order.
- **Hygiene:** 60 tests, clippy 0.

## Round 19 (2026-07-16)
- **Real provider paths now E2E-tested offline (the last big untested
  region).** Two fake local API servers (std-tcp threads serving canned SSE)
  drive the REAL openai-compat and anthropic code paths end to end:
  request shaping (incl. tool-result round trips), SSE parsing, tool-call
  delta accumulation, real `write_file` execution on disk, and the final
  stop. Tests: `fake_openai.rs`, `fake_anthropic.rs`.
  - **Found + fixed a real provider bug:** the OpenAI-compat parser rejected
    the standard `data: [DONE]` stream terminator (`ApiError::Json`); now
    treated as end-of-stream.
  - Clarified API routing: `api:"anthropic"` parses to `Custom` (not
    Anthropic — canonical key is `anthropic-messages`); `Custom` → OpenAI
    compat is intentional and documented by the tests.
  - **Gemini (`google-generative-ai`) added too** (`fake_gemini.rs`): the
    alt=sse candidates/parts/functionCall/usageMetadata dialect, real tool
    execution, functionResponse round trip — all three real provider paths
    now verified offline end to end.
- **Hygiene:** 63 tests, clippy 0.

## Round 20 (2026-07-16)
- **HTTP resilience tested offline:** `retry.rs` fakes a flaky API (500 -> 503
  -> 200) and asserts three POST attempts then a recovered stream; a 400
  fails fast — the backoff/retryable logic in `post_with_retry` is now locked
  in by tests.
- **Full-stack servE E2E (subprocess):** `http_serve.rs` spawns the real
  `pi --serve` binary (via `CARGO_BIN_EXE_pi`), points it at a fake
  openai-compat API, POSTs a prompt over a raw TCP socket, and asserts the
  streamed NDJSON (`write_file` tool trace + `done` frame), that the real
  tool wrote the file to disk, and that the second provider call carries the
  tool result — binary + HTTP + provider + tools in one test.
  - **Found + fixed a real parser bug:** `custom:` model refs split on the
    first `:` (`custom:http://…` became base `"http"`); now splits on the
    last `:` and handles ports/paths. Regression test
    (`custom_model_ref_parses_urls`).
  - `serve()` refactored into `bind_listener`/`serve_listener` for
    testability.
- **Hygiene:** 67 tests, clippy 0.

## Round 21 (2026-07-16)
- **Parallel tool dispatch E2E (`multi_tool.rs`):** one assistant response with
  two `write_file` calls runs both (spawn_blocking fan-out), writes two files,
  and the next provider call carries both tool-role messages.
- **Fallback model E2E (`fallback.rs`):** a primary that always 503s (4
  retries) drops to `fallback_model`, which serves the final text; verified
  primary was attempted and backup served exactly once.
- **Auto-compaction E2E (`compact.rs`):** a `finish_reason:"length"` stop with
  a tight context window triggers summarize + a `Compaction` session entry and
  a `Compacted` event.
- **Cooperative abort E2E (`abort.rs`):** abort stored mid-stream over a slow
  fake server cuts the tail chunk (never delivered) and the turn ends
  promptly (1.5s), with the pre-abort text deltas intact. Debugging surfaced
  the API semantics clearly: `user_turn().await` runs the turn INLINE and
  returns an unbounded replay receiver only after completion; `start_turn`
  spawns its own task (used by the TUI); abort is cooperative, observed at
  the next provider event.
- **Hygiene:** 71 tests, clippy 0.

## Round 22 (2026-07-16)
- **Session persistence round-trip (binary-level, `resume.rs`):** one run
  creates a session file; `--resume <id>` and `--continue` re-attach and
  append to the SAME JSONL file (2 -> 4 -> 6 entries, 3 user turns, fully
  valid JSON). The core "pick up where you left off" story is now locked by
  a test.
- **Stdio RPC protocol test (`rpc_stdio.rs`):** feeds tools/prompt/model/
  quit frames and asserts the ready -> tools_result -> event* -> done ->
  model_result -> bye sequence, plus tool names in tools_result.
- **`--plan` / `-P` (extra over pi):** prepends an explicit plan directive
  so the agent creates a todo-list plan before touching files, then works
  through it. Verified via scenario mock (todo tool called, TODO updated,
  clean exit) and an automated subprocess test (`plan.rs`).
- **Hygiene:** 74 tests, clippy 0.

## Round 23 (2026-07-16)
- **TUI scrollback (PgUp/PgDn):** the transcript view was tail-anchored with
  no way to review older output. Added `draw_markdown_scroll_with` (a scroll
  offset over the tail-anchored renderer), wired PgUp/PgDn in the TUI
  (page-sized steps), auto-reset to the tail on new input, and a unit test
  verifying both the tail-anchored default and scrolled history.
- **`docs/benchmarks.md`:** frozen real numbers — `--version` ~14 ms
  (~900× vs Node ref), `--help` ~15 ms (~1 900×), mock turn ~25 ms, idle
  serve RSS ~1.9 MB (~22× smaller than ref's ~43 MB), static musl 4.15 MB.
  README links it.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 24 (2026-07-16)
- **Verification hardening:** the whole suite now also runs green under the
  RELEASE profile (catches optimization-sensitive timing bugs), and the
  timing-based tests (abort/compact/fallback) were stress-run repeatedly
  under release with zero flakes. `cargo doc --no-deps` is warning-free.
- **`pi --info`:** rich build banner — `pi 0.1.0 · 23 models · 15 tools ·
  rust stable · aarch64` plus provider/toolset summary (clap can't take a
  runtime-computed version string, so this is a dedicated flag).
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 18 (2026-07-16)
- **Shell completions:** `pi --completion bash|zsh|fish` emits a clap-generated
  completion script (new `clap_complete` dep); unsupported shells rejected
  with a clear error.
- **`--list-models --json`** (full catalog as JSON: context/output/tools/
  vision/prices) and **`--doctor --json`** (machine-readable check list) for
  scripting/tooling.
- **Full release verification matrix:** all 11 modes exercised against the
  final release + static binaries — print, json, rpc, mcp-server, completions,
  list-models/doctor/stats/export JSON+HTML, serve healthz, and the
  aarch64-musl static build.
- **Hygiene:** 60 tests, clippy 0.

## Round 19 (2026-07-16)
- **Real provider paths now E2E-tested offline (the last big untested
  region).** Two fake local API servers (std-tcp threads serving canned SSE)
  drive the REAL openai-compat and anthropic code paths end to end:
  request shaping (incl. tool-result round trips), SSE parsing, tool-call
  delta accumulation, real `write_file` execution on disk, and the final
  stop. Tests: `fake_openai.rs`, `fake_anthropic.rs`.
  - **Found + fixed a real provider bug:** the OpenAI-compat parser rejected
    the standard `data: [DONE]` stream terminator (`ApiError::Json`); now
    treated as end-of-stream.
  - Clarified API routing: `api:"anthropic"` parses to `Custom` (not
    Anthropic — canonical key is `anthropic-messages`); `Custom` → OpenAI
    compat is intentional and documented by the tests.
  - **Gemini (`google-generative-ai`) added too** (`fake_gemini.rs`): the
    alt=sse candidates/parts/functionCall/usageMetadata dialect, real tool
    execution, functionResponse round trip — all three real provider paths
    now verified offline end to end.
- **Hygiene:** 63 tests, clippy 0.

## Round 20 (2026-07-16)
- **HTTP resilience tested offline:** `retry.rs` fakes a flaky API (500 -> 503
  -> 200) and asserts three POST attempts then a recovered stream; a 400
  fails fast — the backoff/retryable logic in `post_with_retry` is now locked
  in by tests.
- **Full-stack servE E2E (subprocess):** `http_serve.rs` spawns the real
  `pi --serve` binary (via `CARGO_BIN_EXE_pi`), points it at a fake
  openai-compat API, POSTs a prompt over a raw TCP socket, and asserts the
  streamed NDJSON (`write_file` tool trace + `done` frame), that the real
  tool wrote the file to disk, and that the second provider call carries the
  tool result — binary + HTTP + provider + tools in one test.
  - **Found + fixed a real parser bug:** `custom:` model refs split on the
    first `:` (`custom:http://…` became base `"http"`); now splits on the
    last `:` and handles ports/paths. Regression test
    (`custom_model_ref_parses_urls`).
  - `serve()` refactored into `bind_listener`/`serve_listener` for
    testability.
- **Hygiene:** 67 tests, clippy 0.

## Round 21 (2026-07-16)
- **Parallel tool dispatch E2E (`multi_tool.rs`):** one assistant response with
  two `write_file` calls runs both (spawn_blocking fan-out), writes two files,
  and the next provider call carries both tool-role messages.
- **Fallback model E2E (`fallback.rs`):** a primary that always 503s (4
  retries) drops to `fallback_model`, which serves the final text; verified
  primary was attempted and backup served exactly once.
- **Auto-compaction E2E (`compact.rs`):** a `finish_reason:"length"` stop with
  a tight context window triggers summarize + a `Compaction` session entry and
  a `Compacted` event.
- **Cooperative abort E2E (`abort.rs`):** abort stored mid-stream over a slow
  fake server cuts the tail chunk (never delivered) and the turn ends
  promptly (1.5s), with the pre-abort text deltas intact. Debugging surfaced
  the API semantics clearly: `user_turn().await` runs the turn INLINE and
  returns an unbounded replay receiver only after completion; `start_turn`
  spawns its own task (used by the TUI); abort is cooperative, observed at
  the next provider event.
- **Hygiene:** 71 tests, clippy 0.

## Round 22 (2026-07-16)
- **Session persistence round-trip (binary-level, `resume.rs`):** one run
  creates a session file; `--resume <id>` and `--continue` re-attach and
  append to the SAME JSONL file (2 -> 4 -> 6 entries, 3 user turns, fully
  valid JSON). The core "pick up where you left off" story is now locked by
  a test.
- **Stdio RPC protocol test (`rpc_stdio.rs`):** feeds tools/prompt/model/
  quit frames and asserts the ready -> tools_result -> event* -> done ->
  model_result -> bye sequence, plus tool names in tools_result.
- **`--plan` / `-P` (extra over pi):** prepends an explicit plan directive
  so the agent creates a todo-list plan before touching files, then works
  through it. Verified via scenario mock (todo tool called, TODO updated,
  clean exit) and an automated subprocess test (`plan.rs`).
- **Hygiene:** 74 tests, clippy 0.

## Round 23 (2026-07-16)
- **TUI scrollback (PgUp/PgDn):** the transcript view was tail-anchored with
  no way to review older output. Added `draw_markdown_scroll_with` (a scroll
  offset over the tail-anchored renderer), wired PgUp/PgDn in the TUI
  (page-sized steps), auto-reset to the tail on new input, and a unit test
  verifying both the tail-anchored default and scrolled history.
- **`docs/benchmarks.md`:** frozen real numbers — `--version` ~14 ms
  (~900× vs Node ref), `--help` ~15 ms (~1 900×), mock turn ~25 ms, idle
  serve RSS ~1.9 MB (~22× smaller than ref's ~43 MB), static musl 4.15 MB.
  README links it.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 24 (2026-07-16)
- **Verification hardening:** the whole suite now also runs green under the
  RELEASE profile (catches optimization-sensitive timing bugs), and the
  timing-based tests (abort/compact/fallback) were stress-run repeatedly
  under release with zero flakes. `cargo doc --no-deps` is warning-free.
- **`pi --info`:** rich build banner — `pi 0.1.0 · 23 models · 15 tools ·
  rust stable · aarch64` plus provider/toolset summary (clap can't take a
  runtime-computed version string, so this is a dedicated flag).
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 17 (2026-07-16)
- **Graceful TUI exit (terminal safety bug fixed):** `/exit` and idle Ctrl-C
  called `std::process::exit(0)`, which skips destructors and left the
  terminal in raw mode / alternate screen. Refactored to an outcome enum
  (`CmdOutcome::None|Prompt|Exit`) bubbling through `dispatch_input`/
  `submit_input`/queued-drain into a labelled `'main` loop that breaks and
  runs `Tui::stop()` (LeaveAlternateScreen + cursor-show + raw-mode off).
  PTY-verified: both `/exit` and Ctrl-C now emit `\e[?1049l` on the way out.
- **`-i "prompt"` first-message + trailing-var-arg fix** (from round 16) and
  **machine-readable outputs:** `--list-models --json`, `--doctor --json`,
  `--stats --json`.
- **Hygiene:** 60 tests, clippy 0.

## Round 19 (2026-07-16)
- **Real provider paths now E2E-tested offline (the last big untested
  region).** Two fake local API servers (std-tcp threads serving canned SSE)
  drive the REAL openai-compat and anthropic code paths end to end:
  request shaping (incl. tool-result round trips), SSE parsing, tool-call
  delta accumulation, real `write_file` execution on disk, and the final
  stop. Tests: `fake_openai.rs`, `fake_anthropic.rs`.
  - **Found + fixed a real provider bug:** the OpenAI-compat parser rejected
    the standard `data: [DONE]` stream terminator (`ApiError::Json`); now
    treated as end-of-stream.
  - Clarified API routing: `api:"anthropic"` parses to `Custom` (not
    Anthropic — canonical key is `anthropic-messages`); `Custom` → OpenAI
    compat is intentional and documented by the tests.
  - **Gemini (`google-generative-ai`) added too** (`fake_gemini.rs`): the
    alt=sse candidates/parts/functionCall/usageMetadata dialect, real tool
    execution, functionResponse round trip — all three real provider paths
    now verified offline end to end.
- **Hygiene:** 63 tests, clippy 0.

## Round 20 (2026-07-16)
- **HTTP resilience tested offline:** `retry.rs` fakes a flaky API (500 -> 503
  -> 200) and asserts three POST attempts then a recovered stream; a 400
  fails fast — the backoff/retryable logic in `post_with_retry` is now locked
  in by tests.
- **Full-stack servE E2E (subprocess):** `http_serve.rs` spawns the real
  `pi --serve` binary (via `CARGO_BIN_EXE_pi`), points it at a fake
  openai-compat API, POSTs a prompt over a raw TCP socket, and asserts the
  streamed NDJSON (`write_file` tool trace + `done` frame), that the real
  tool wrote the file to disk, and that the second provider call carries the
  tool result — binary + HTTP + provider + tools in one test.
  - **Found + fixed a real parser bug:** `custom:` model refs split on the
    first `:` (`custom:http://…` became base `"http"`); now splits on the
    last `:` and handles ports/paths. Regression test
    (`custom_model_ref_parses_urls`).
  - `serve()` refactored into `bind_listener`/`serve_listener` for
    testability.
- **Hygiene:** 67 tests, clippy 0.

## Round 21 (2026-07-16)
- **Parallel tool dispatch E2E (`multi_tool.rs`):** one assistant response with
  two `write_file` calls runs both (spawn_blocking fan-out), writes two files,
  and the next provider call carries both tool-role messages.
- **Fallback model E2E (`fallback.rs`):** a primary that always 503s (4
  retries) drops to `fallback_model`, which serves the final text; verified
  primary was attempted and backup served exactly once.
- **Auto-compaction E2E (`compact.rs`):** a `finish_reason:"length"` stop with
  a tight context window triggers summarize + a `Compaction` session entry and
  a `Compacted` event.
- **Cooperative abort E2E (`abort.rs`):** abort stored mid-stream over a slow
  fake server cuts the tail chunk (never delivered) and the turn ends
  promptly (1.5s), with the pre-abort text deltas intact. Debugging surfaced
  the API semantics clearly: `user_turn().await` runs the turn INLINE and
  returns an unbounded replay receiver only after completion; `start_turn`
  spawns its own task (used by the TUI); abort is cooperative, observed at
  the next provider event.
- **Hygiene:** 71 tests, clippy 0.

## Round 22 (2026-07-16)
- **Session persistence round-trip (binary-level, `resume.rs`):** one run
  creates a session file; `--resume <id>` and `--continue` re-attach and
  append to the SAME JSONL file (2 -> 4 -> 6 entries, 3 user turns, fully
  valid JSON). The core "pick up where you left off" story is now locked by
  a test.
- **Stdio RPC protocol test (`rpc_stdio.rs`):** feeds tools/prompt/model/
  quit frames and asserts the ready -> tools_result -> event* -> done ->
  model_result -> bye sequence, plus tool names in tools_result.
- **`--plan` / `-P` (extra over pi):** prepends an explicit plan directive
  so the agent creates a todo-list plan before touching files, then works
  through it. Verified via scenario mock (todo tool called, TODO updated,
  clean exit) and an automated subprocess test (`plan.rs`).
- **Hygiene:** 74 tests, clippy 0.

## Round 23 (2026-07-16)
- **TUI scrollback (PgUp/PgDn):** the transcript view was tail-anchored with
  no way to review older output. Added `draw_markdown_scroll_with` (a scroll
  offset over the tail-anchored renderer), wired PgUp/PgDn in the TUI
  (page-sized steps), auto-reset to the tail on new input, and a unit test
  verifying both the tail-anchored default and scrolled history.
- **`docs/benchmarks.md`:** frozen real numbers — `--version` ~14 ms
  (~900× vs Node ref), `--help` ~15 ms (~1 900×), mock turn ~25 ms, idle
  serve RSS ~1.9 MB (~22× smaller than ref's ~43 MB), static musl 4.15 MB.
  README links it.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 24 (2026-07-16)
- **Verification hardening:** the whole suite now also runs green under the
  RELEASE profile (catches optimization-sensitive timing bugs), and the
  timing-based tests (abort/compact/fallback) were stress-run repeatedly
  under release with zero flakes. `cargo doc --no-deps` is warning-free.
- **`pi --info`:** rich build banner — `pi 0.1.0 · 23 models · 15 tools ·
  rust stable · aarch64` plus provider/toolset summary (clap can't take a
  runtime-computed version string, so this is a dedicated flag).
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 18 (2026-07-16)
- **Shell completions:** `pi --completion bash|zsh|fish` emits a clap-generated
  completion script (new `clap_complete` dep); unsupported shells rejected
  with a clear error.
- **`--list-models --json`** (full catalog as JSON: context/output/tools/
  vision/prices) and **`--doctor --json`** (machine-readable check list) for
  scripting/tooling.
- **Full release verification matrix:** all 11 modes exercised against the
  final release + static binaries — print, json, rpc, mcp-server, completions,
  list-models/doctor/stats/export JSON+HTML, serve healthz, and the
  aarch64-musl static build.
- **Hygiene:** 60 tests, clippy 0.

## Round 19 (2026-07-16)
- **Real provider paths now E2E-tested offline (the last big untested
  region).** Two fake local API servers (std-tcp threads serving canned SSE)
  drive the REAL openai-compat and anthropic code paths end to end:
  request shaping (incl. tool-result round trips), SSE parsing, tool-call
  delta accumulation, real `write_file` execution on disk, and the final
  stop. Tests: `fake_openai.rs`, `fake_anthropic.rs`.
  - **Found + fixed a real provider bug:** the OpenAI-compat parser rejected
    the standard `data: [DONE]` stream terminator (`ApiError::Json`); now
    treated as end-of-stream.
  - Clarified API routing: `api:"anthropic"` parses to `Custom` (not
    Anthropic — canonical key is `anthropic-messages`); `Custom` → OpenAI
    compat is intentional and documented by the tests.
  - **Gemini (`google-generative-ai`) added too** (`fake_gemini.rs`): the
    alt=sse candidates/parts/functionCall/usageMetadata dialect, real tool
    execution, functionResponse round trip — all three real provider paths
    now verified offline end to end.
- **Hygiene:** 63 tests, clippy 0.

## Round 20 (2026-07-16)
- **HTTP resilience tested offline:** `retry.rs` fakes a flaky API (500 -> 503
  -> 200) and asserts three POST attempts then a recovered stream; a 400
  fails fast — the backoff/retryable logic in `post_with_retry` is now locked
  in by tests.
- **Full-stack servE E2E (subprocess):** `http_serve.rs` spawns the real
  `pi --serve` binary (via `CARGO_BIN_EXE_pi`), points it at a fake
  openai-compat API, POSTs a prompt over a raw TCP socket, and asserts the
  streamed NDJSON (`write_file` tool trace + `done` frame), that the real
  tool wrote the file to disk, and that the second provider call carries the
  tool result — binary + HTTP + provider + tools in one test.
  - **Found + fixed a real parser bug:** `custom:` model refs split on the
    first `:` (`custom:http://…` became base `"http"`); now splits on the
    last `:` and handles ports/paths. Regression test
    (`custom_model_ref_parses_urls`).
  - `serve()` refactored into `bind_listener`/`serve_listener` for
    testability.
- **Hygiene:** 67 tests, clippy 0.

## Round 21 (2026-07-16)
- **Parallel tool dispatch E2E (`multi_tool.rs`):** one assistant response with
  two `write_file` calls runs both (spawn_blocking fan-out), writes two files,
  and the next provider call carries both tool-role messages.
- **Fallback model E2E (`fallback.rs`):** a primary that always 503s (4
  retries) drops to `fallback_model`, which serves the final text; verified
  primary was attempted and backup served exactly once.
- **Auto-compaction E2E (`compact.rs`):** a `finish_reason:"length"` stop with
  a tight context window triggers summarize + a `Compaction` session entry and
  a `Compacted` event.
- **Cooperative abort E2E (`abort.rs`):** abort stored mid-stream over a slow
  fake server cuts the tail chunk (never delivered) and the turn ends
  promptly (1.5s), with the pre-abort text deltas intact. Debugging surfaced
  the API semantics clearly: `user_turn().await` runs the turn INLINE and
  returns an unbounded replay receiver only after completion; `start_turn`
  spawns its own task (used by the TUI); abort is cooperative, observed at
  the next provider event.
- **Hygiene:** 71 tests, clippy 0.

## Round 22 (2026-07-16)
- **Session persistence round-trip (binary-level, `resume.rs`):** one run
  creates a session file; `--resume <id>` and `--continue` re-attach and
  append to the SAME JSONL file (2 -> 4 -> 6 entries, 3 user turns, fully
  valid JSON). The core "pick up where you left off" story is now locked by
  a test.
- **Stdio RPC protocol test (`rpc_stdio.rs`):** feeds tools/prompt/model/
  quit frames and asserts the ready -> tools_result -> event* -> done ->
  model_result -> bye sequence, plus tool names in tools_result.
- **`--plan` / `-P` (extra over pi):** prepends an explicit plan directive
  so the agent creates a todo-list plan before touching files, then works
  through it. Verified via scenario mock (todo tool called, TODO updated,
  clean exit) and an automated subprocess test (`plan.rs`).
- **Hygiene:** 74 tests, clippy 0.

## Round 23 (2026-07-16)
- **TUI scrollback (PgUp/PgDn):** the transcript view was tail-anchored with
  no way to review older output. Added `draw_markdown_scroll_with` (a scroll
  offset over the tail-anchored renderer), wired PgUp/PgDn in the TUI
  (page-sized steps), auto-reset to the tail on new input, and a unit test
  verifying both the tail-anchored default and scrolled history.
- **`docs/benchmarks.md`:** frozen real numbers — `--version` ~14 ms
  (~900× vs Node ref), `--help` ~15 ms (~1 900×), mock turn ~25 ms, idle
  serve RSS ~1.9 MB (~22× smaller than ref's ~43 MB), static musl 4.15 MB.
  README links it.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 24 (2026-07-16)
- **Verification hardening:** the whole suite now also runs green under the
  RELEASE profile (catches optimization-sensitive timing bugs), and the
  timing-based tests (abort/compact/fallback) were stress-run repeatedly
  under release with zero flakes. `cargo doc --no-deps` is warning-free.
- **`pi --info`:** rich build banner — `pi 0.1.0 · 23 models · 15 tools ·
  rust stable · aarch64` plus provider/toolset summary (clap can't take a
  runtime-computed version string, so this is a dedicated flag).
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 25 (2026-07-16)
- **Disk headroom:** `target/debug/deps` (3 GB of rebuildable artifacts)
  pruned; workspace back to 3.9 GB free. Full suite re-verified from a cold
  cache (75 passed) — also serves as a from-scratch compile check.
- **`docs/comparison.md`:** feature-parity table vs the Node reference plus
  the "beyond the reference" list (MCP server, extensions, scenario tests,
  static musl, completions, tree/GC/export, serve-token, --info, images);
  README links it alongside benchmarks.
- **`pi doctor` extended:** now checks `.pi/pi.json` parses and that the
  session dir is writable (in addition to binaries/dir/git/disk/tty/keys).
  Verified: absent pi.json -> ok(optional); session dir -> writable.
- **Hygiene:** 75 tests, clippy 0.

## Round 26 (2026-07-16)
- **Session durability test:** full round trip (roles user/assistant/tool
  w/ tool-call ids, a fork, a compaction) survives write -> reload with
  identical ids/parents/timestamps; fork parent edge and head-chain
  exclusion verified.
- **README accuracy pass:** "Testing & hardening" rewritten to match reality
  (76 tests, fake-provider E2E suite, binary-level tests, PTY coverage) and
  "Known limits" corrected (commercial accounts unreachable from the
  sandbox but wire protocols covered offline).
- **Hygiene:** 76 tests, clippy 0.

## Round 27 (2026-07-16)
- **Session labeling (`/label <name>`):** the dormant `Label` body type is now
  real: `Session::push_label` + `Session::label()`, a TUI `/label <name>`
  command, and a `label` column in `--sessions` (and the TUI `/sessions`).
  PTY-verified end to end; labels survive reload (new test +
  `session_label_round_trip`).
- **Hygiene:** 77 tests, clippy 0.

## Round 28 (2026-07-16)
- **Concurrent serve test (`serve_concurrent.rs`):** five simultaneous POST
  /rpc clients against the real binary — every client gets its done frame
  and model text, and the fake provider serves exactly one call per client
  (turns serialize behind the agent mutex, no hangs, no corrupency).
- **CLI error-path tests (`cli_errors.rs`):** unknown models, missing files,
  and malformed RPC lines fail with clear messages (never panics).
- **Hygiene:** 81 tests, clippy 0.

## Round 29 (2026-07-16)
- **config + export unit tests (were untested modules):** FileConfig parses
  every field (with unknown fields ignored) and errors on malformed input
  naming the file; markdown export contains user/assistant/tool turns and
  tool-call traces; HTML export is standalone + escaped. Found + fixed a
  test bug (the `"#` sequence in a raw string closed `r#"…"#` early) and a
  genuine parallel-test race (two export tests shared one temp dir).
- **Hygiene:** 86 tests, clippy 0.

## Round 30 (2026-07-16)
- **CLI `--fallback` E2E (`fallback_cli.rs`):** a fake primary that always
  503s plus `--fallback custom:...:backup` — the binary produces the backup's
  text, proving the flag -> config -> fallback chain at the command line.
- **Agentic diff (`agent_diff.rs`):** a scenario drives the agent to apply a
  real unified diff via `apply_diff`; the file on disk changes and the final
  confirmation text arrives — the diff engine exercised through the full
  agent loop.
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 31 (2026-07-16)
- **Panic audit + hardening:** swept every `.unwrap()/expect()` in production
  (non-test) code. Core crates were already clean except two defensive spots
  now fixed: session JSON serialization uses `if let Ok(...)` (flush +
  save_full) and the TUI replaces two `agent.take().unwrap()` sites with
  graceful "busy, try again" paths. The only remaining `expect` is the
  compile-time embedded models.json (build-time invariant).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Round 32 (2026-07-16)
- **TUI SIGTERM handling (terminal safety):** a supervisor killing a session
  no longer leaves the pty in raw mode. A tokio unix signal listener feeds a
  watch channel the sync event loop checks each cycle; on SIGTERM the loop
  exits through `Tui::stop()` (LeaveAlternateScreen + cursor + raw-mode off).
  Verified live: real pi process survives the signal ~400ms, restores the
  terminal, no panic.
- **`pi export` now stamps the session label** (`# session <id> (**<label>**)`).
- **Hygiene:** 88 tests, clippy 0.

## Round 33 (2026-07-16)
- **`--info --json`:** machine-readable build banner (version/rust/arch,
  23 models, 15 tools, provider list).
- **Serve auth automated (`serve_token.rs`):** the real binary enforces
  `--serve-token` — `/healthz` stays open (200), `/info` + `/sessions`
  return 401 without / with a wrong token and 200 with the right one.
- **Verified:** the HTTP rpc path already supports model switching and
  tools over POST /rpc (shared rpc::serve arms).
- **Hygiene:** 89 tests, clippy 0.

## Round 34 (2026-07-16)
- **Live status line in the TUI:** the spinner row now shows
  `model  ·  session <id>` (plus `^scroll` while scrolled back),
  dim-colored via the theme; `Spinner::width()` computes the exact glyph
  width so the suffix aligns. PTY-verified.
- **Hygiene:** 90 tests, clippy 0.

## Round 35 (2026-07-16)
- **`--sessions --json`:** machine-readable session directory listing (id,
  label, entries, messages, bytes, mtime).
- **Per-call bash timeout verified (`bash_timeout.rs`):** a scenario issues
  `bash {command: "sleep 5 …", timeout_ms: 300}` — the tool is killed at
  ~300ms (0.41 s total), reports the timeout, and the run completes. The
  per-call `timeout_ms` override is locked by a test.
- **Hygiene:** 92 tests, clippy 0.

## Rounds 93–135 (compacted record — the log tail was restructured and later appends missed)

### 93–100: parity close-out + milestone
- 93: CI readiness — adaptive build.sh (in-tree toolchain or system cargo), corrected ci.yml, MSRV 1.88 declared.
- 94: --info + session_dir; CI-style clone simulation exposed the missing MSRV.
- 95: REPL: /theme live switching; 96: browser dashboard (GET /) with streaming rpc.
- 97: MCP client binary E2E; 98: tool-timeout E2E; 99: usage entry persistence + --cost; 100: O(n) branch() (~15× on 3000-entry stats).

### 101–110: operator surface
- 101: unified stats (run/run_json share one counter); serial-suite verification.
- 102: user-journey tour.sh (12 checks) + git init (real repo, .gitignore, commits).
- 103: --gc --gc-dry-run; git-backed --review tests.
- 104: adversarial HTTP batch + suite flake audits.
- 105: git as real history (self-hosted --review); 106: --cost --json + /cost endpoint + lock-free routes.
- 107: external abort {"type":"abort"} + dashboard stop button (contract-locked).
- 108: consumer bundle simulation — portable verify/tour binary resolution (27/27 from a bare dist copy).
- 109: tool-layer security round (audit 2): git --output escape, glob/grep root escapes, symlink escape, bash SIGPIPE + hard timeout, apply_diff hunk verification, timeout clamp; 3 security tests.
- 110: provider-layer security round (audit 3): conservative transport retry (no double-charge), SSE caps, thinking-budget clamp, usage accounting, cleartext-URL guard; empty-frame tolerance; deterministic tool order.

### 111–120: hardening close-out
- 111: per-turn serve console log; static artifact rebuilt with security fixes.
- 112: session-dir consistency (pi.json session_dir honored everywhere); 3rd audit (TUI/CLI).
- 113: TUI sanitizer (model ESC/control bytes), control chords + Ctrl-D exit, 2nd-Ctrl-C force-quit; 114: bracketed paste, mode-conflict detection, honest exit codes.
- 115: transcript bound (2 MB, elided marker); 116: size-fail fallback + wide-char columns.
- 117: session-dir threaded into the TUI + agent (full consistency, relative→root join).
- 118: four-audit milestone — zero CRITICALs outstanding.
- 119: markdown link off-by-one + SGR full-style emission; 120: build provenance (--info build_sha, -V banner).

## Round 137 (2026-07-16) — record repair
- Found that rounds 93–135 had silently missed the narrative log (the tail
  had been restructured); inserted an accurate compacted record above.
  From now on every PROGRESS write is asserted.
- State: 128 tests, clippy 0, 28/28 sign-off, 12/12 tour, four audits closed,
  build provenance live, git history + releases current.

## Round 138 (2026-07-16)
- **Idle-serve leak check:** RSS stable at ~2.1 MB over 60 s idle (settling
  to 1.0 MB) — no memory leak in the long-lived worker. Suite re-confirmed
  128/128, clippy 0.

## Round 139 (2026-07-16)
- **Dependency inventory audit:** 13 direct deps, all mainstream; 171 locked
  packages; no exotic/unsafe-leaning crates. Documented in README.
- **Hygiene:** 128 tests, clippy 0.

## Round 140 (2026-07-16)
- **v1.0.0 release tag** at the fully-hardened state (128 tests, 28/28
  sign-off, four audits closed); dist VERSION carries both the version and
  the full build banner (23 models · 15 tools · sha).

## Round 141 (2026-07-16)
- **v1.0.0 tag verified as a release:** tagged source tree is identical to
  HEAD, builds clean from a fresh checkout with the pinned toolchain (the
  resulting binary reports the full banner). The milestone is reproducible.

## Round 142 (2026-07-16)
- **Published to GitHub:** pushed the full history (39 commits, branch
  master @ f538a1b) and the v1.0.0 tag to
  https://github.com/CarbonWalls/pi-full-rewrite (SSH authentication via
  the host's ed25519 key). Remote HEAD verified to match local. The
  repo's CI (tests + clippy + 28-point sign-off + static musl) will run
  against the pushed state.

## Round 143 (2026-07-16)
- **GitHub tag-release workflow** (`.github/workflows/release.yml`): on every
  `v*` tag push, CI runs the full verification (tests, clippy, sign-off),
  builds glibc + static musl binaries, bundles dist/, and publishes a
  GitHub Release with the binaries. The published repo now self-releases.

## Round 144 (2026-07-16)
- **CI refinement:** every GitHub Actions build stamps PI_BUILD_SHA (so the
  shipped binary always names its source commit), and the sign-off job
  guards with `git diff --exit-code` that no build artifact leaks into the
  tree. Repo bloat-checked: 145 tracked files, no toolchain/targets tracked.

## Round 145 (2026-07-16)
- **Remote installer for the published repo:** `scripts/install-remote.sh`
  downloads the static binary from the latest GitHub release
  (pi-linux-amd64-static), verifies it is statically linked, and installs
  to ~/.local/bin — plus a README one-liner for GitHub visitors.

## Round 146 (2026-07-16)
- **CI on GitHub was failing** ("Workspace tests" + "static ELF" checks).
  Root cause narrowed via fresh-clone reproductions: local clone passes are
  green; failures are runner-environment/timing sensitive. CI hardened:
  serial-ish test threads, `--no-fail-fast`, RUST_BACKTRACE, and failures
  now surface as `::error::` GitHub annotations (readable via the public
  API without a token) so the exact failing test is identifiable.

## Round 147 (2026-07-16)
- **CI root cause narrowed:** failed runs last 44 s / 90 s — far too short
  for a cold compile, so both jobs DIE AT COMPILE TIME on the runner's
  `@stable` toolchain (differs from the dev-pinned 1.99). Fix: pin CI to
  `dtolnay/rust-toolchain@1.99.0` + `--locked` everywhere. The WSL box was
  offered (x86_64, no toolchain) — machine rejected only as a repo target,
  not needed for this fix since GitHub runners are amd64.

## Round 148 (2026-07-16)
- **CI root cause FOUND + FIXED:** the failing test was
  `feature_tests::review_prompt_contains_git_sessions` — its `git commit`
  needs an identity, and **GitHub runners ship no git identity** (my host
  has one, so it only failed in CI). Reproduced under `GIT_CONFIG_NOSYSTEM=1
  HOME=/tmp/emptyhome` (runner-identical), fixed the test to set
  user.name/user.email hermetically, and confirmed 128/128 under the strict
  env. All other git tests were already hermetic.

## Round 149 (2026-07-16)
- Also instrumented the static-musl job: if `file` doesn't say "statically
  linked", CI now reports exactly what file(1) said (the musl output differs
  between plain `musl-gcc` and the clang wrapper used locally). Watching the
  next run for both jobs.

## Round 150 (2026-07-16)
- **CI green-up (part 2):** static-musl job now PASSES — the binary was
  always fine; `file(1)` says `static-pie linked`, not `statically linked`
  (pattern fixed). Test job failures fully diagnosed via API-readable
  annotations: (a) `git commit` needs identity on sign-off runners and one
  more test (tools.rs:244) lacked it — fixed hermetically; (b) the two bash
  timeout tests fail only on the shared runner — added a `/proc`-based
  **descendant-tree reaper** to the timeout kill path (covers job-control
  sub-groups that the group-kill can miss; verified group-kill semantics are
  identical on real Linux, incl. the WSL amd64 box) and made the per-call
  override e2e tolerate cold-runner startup while still failing loudly if
  the override is ignored. Clippy 0, tests 128.

## Round 151 (2026-07-16)
- CI bash-timeout wrangling: `bash_per_call_timeout_override` passes with
  the startup-tolerant form; `bash_timeout_kills_process_group` still fails
  on the runner (passes on real Linux incl. WSL). Added a `pkill -9 -P`
  direct-children net to the reaper and made the test check the grandchild's
  actual `comm` (a recycled pid must not fake it alive) — the failure message
  now reports the real comm for final diagnosis.

## Round 152 (2026-07-16)
- **Kill-order bug found (real, user-facing):** the timeout path killed the
  shell FIRST; the shell's death reparents its children to init, so the
  ancestry reaper and `pkill -P` then miss them — stranding background
  grandchildren (exactly what the runner reproduced). Reordered to reap
  descendants FIRST, then the group, then the shell. This also fixes
  job-control shells (sub-groups) for real users. 128 tests, clippy 0.

## Round 153 (2026-07-16) — **CI GREEN on GitHub Actions**
- Full workflow now passes: `test` (128 tests serial, clippy 0, sign-off
  28/28) and `static-musl` (static-pie ELF verified). The runner surfaced a
  real bug — bash-timeout orphans when the shell dies first and its children
  get reparented to init — now fixed for every user, not just CI.
- v1.0.0 moved to the green commit so the tag-release workflow builds its
  GitHub Release from the verified state.

## Round 1 (goal restored) — release asset verified end to end
- Downloaded `pi-linux-amd64-static` from the live GitHub release and ran it
  on the real amd64 Debian/WSL box: ELF64 x86_64, no interpreter (static),
  banner reports build sha b0b0220 (== tag commit). Source → CI → tag →
  release → verified binary: the full loop is closed.
- Diagnose job (CI debugging aid) is now manual (`workflow_dispatch`).

## Round 2 — cross-arch release verification + benchmarks
- Both release assets (`pi-linux-amd64` glibc and `-static`) verified on the
  real amd64 Debian/WSL box: full offline agent loop works (mock turn →
  session wrote), provenance banner correct. Benchmarks on real hardware:
  `--version` **2 ms**, full mock turn **6 ms** (vs 15/44 ms on the aarch64
  dev host) — recorded in docs/benchmarks.md.

## Round 3 — bg_kill reap-first + arm64 release asset
- `bg_kill` now uses the same kill ordering as bash.rs (descendant tree →
  group → child) via the shared `reap_process_tree` — background jobs can no
  longer orphan grandchildren on timeout/kill, on any shell.
- The tag-release workflow now also ships `pi-linux-arm64-static` (aarch64
  musl) — the installer already handles that asset name. 128 tests, clippy 0.

## Round 4 — v1.0.1 released with verified binaries
- Tagged v1.0.1 → full release pipeline ran green: CI passed from the tag,
  and a live GitHub Release now carries pi-linux-amd64 + pi-linux-amd64-static
  + README + VERSION (ELF64 x86_64 verified). Learned: the runner's musl-gcc
  cannot cross-link aarch64 (x86_64-only), so the arm64 static binary stays a
  local build step (noted in the workflow).

## Round 5 — static rebuilt + README badges
- Static aarch64 binary rebuilt with the kill-order fixes (4,656,760 bytes);
  dist refreshed (glibc + static).
- README now carries live CI + release badges for the public repo.

## Round 6 — reaper walk-first ordering + regression tests
- Reordering the reaper inside: the /proc ancestry walk now runs BEFORE the
  `pkill -P` net — pkill first would reparent deeper descendants to init and
  hide them from the walk. Two regression tests lock it: (1) a job-control
  sub-group child (`setsid sleep &`, own pgid) is reaped; (2) a deep grandchild
  under an intermediate shell is reached by the ancestry walk (pids captured
  via `$!` pid files — this sandbox hides spawned children from /proc
  enumeration, which the tests now avoid). 130 tests, clippy 0.

## Round 7 — rebuilds + cross-confirmation
- Static binary (4,656,760 B) rebuilt with the reaper-ordering change; dist
  refreshed. **CI on GitHub passed with the two new reaper regression tests**
  (the runner-safe SIGKILL/setsid paths) — the fixes hold on both platforms.

## Round 8 — v1.0.2
- Tagged v1.0.2 at the current green state so the release ships the
  reaper-ordering fixes; CHANGELOG documents 1.0.2. Watching the release
  build, then verifying the assets.

## Round 9 — docs completeness
- ARCHITECTURE.md gained a "Releases & CI (GitHub)" section documenting the
  whole pipeline: pinned-toolchain CI → tag-triggered release with three
  verified assets → one-line remote installer; also the hermetic-identity
  and reap-first notes for future maintainers. (The WSL box was offline for
  a remote one-liner acceptance — re-attempt later.)

## Round 10 — arm64 coverage in releases
- The tag-release workflow now also cross-builds a GLIBC arm64 binary
  (`pi-linux-arm64` via gcc-aarch64-linux-gnu; musl-aarch64 still can't
  cross-link — that static binary stays a local-aarch64-host build).
  `install-remote.sh` now serves aarch64 the glibc asset. Verified on the
  next tag push (v1.0.3).

## Round 11 — arm64 cross-link finding
- The runner's `aarch64-unknown-linux-gnu` cross-link fails (`gcc-aarch64-
  linux-gnu` alone is insufficient); the release workflow now makes arm64 an
  OPTIONAL asset that warns and proceeds — v1.0.3 ships amd64 binaries
  (verified downloadable) and never blocks on the optional build.
- `install-remote.sh` maps aarch64 back to `pi-linux-arm64-static` (the
  binary actually produced on an aarch64 host; attachable to any release).

## Round 12 — doc counts current
- README + RELEASING refreshed to the true "130 tests" (was 128). Local
  suite re-confirmed 130/130, clippy 0. (The WSL box stayed offline; the
  remote one-liner acceptance re-attempts whenever it is back.)

## Round 13 — local capstone at current pushed state
- Full local gates on the current HEAD: 130 tests, clippy 0, sign-off 28/28,
  user journey 12/12, `--version` ~19 ms. All green. (GH API rate-limited
  and the WSL box offline this round — the remote checks resume when they
  are back; the local gate is the source of truth meanwhile.)

## Round 14 — repo hygiene: session data was in git
- Dogfooding `pi --review` on the repo exposed that **11 session JSONLs had
  been committed to master** and `.pi/` was not gitignored. Fixed: `.pi/`
  ignored, files untracked (history keeps them, working tree clean), and CI
  now fails if any `/.pi/` path is ever tracked again. Suite re-confirmed
  130. (GH API + WSL box both unavailable this round.)

## Round 15 — CI-command + README example replication (offline gates)
- The exact CI test invocation (`--no-fail-fast -- --test-threads=1`) passes
  130/130 locally, the hygiene guard passes, and the README's documented
  one-shot command works verbatim. External channels (GH API + WSL) still
  down; all local truth verified.

## Round 16 — export path verified
- Offline functional checks: sessions persist (quiet one-shot), `--export
  <jsonl> --format txt` renders proper markdown, `--export-all` writes
  `<id>.md` files. (My first `--export` attempt used the wrong arg shape —
  the CLI's own --help shows the correct schema; both flags work as
  documented.)

## Round 17 — dashboard verified + verification snapshot
- Offline: /healthz 200, dashboard dashboard title renders, /cost and
  /sessions return correct JSON. Added docs/verification-snapshot.md — a
  release-readable matrix of the exact current gates (130 tests, 28/28, 12/12,
  CI passing, releases, benchmarks, maintainer notes).

## Round 18 — adversarial no-panic sweep (release binary)
- Ran 9 hostile CLI invocations against the release binary with
  RUST_BACKTRACE=1: bad flags, missing root, garbage scenario, empty session
  dirs, --continue with no sessions, empty prompts, json-ask without prompt.
  **Zero panics** — every case exits cleanly (0/1/2 as appropriate). The
  robust-exit discipline holds under adversarial input.

## Round 19 — session lifecycle E2E
- Created 3 sessions; `--resume <id>` grew the SAME file 3→6 entries (id
  preserved, no new file); `--gc --gc-dry-run` predicted exactly the file
  that the real GC removed; final state 2 files. Dry-run and real GC agree.

## Round 20 — remote channels back; CI confirmed green
- The GitHub API recovered after its rate window: **CI has passed for the
  last four commits** — including the current HEAD with the `.pi` hygiene
  guard. Releases v1.0.0–v1.0.3 all present with the verified asset sets.
  The WSL box stayed offline for the one-liner acceptance (7th attempt).

## Round 21 — newest commit CI green
- The freshly pushed commit (28a062a, round-20 record) passed CI
  (test + static-musl jobs). CI is now green across five consecutive
  commits. WSL remains offline (the one-liner acceptance stays pending).

## Round 22 — benchmark refresh
- Current binary measured: `--version` ~15 ms, full mock turn ~24 ms (best of
  5) — the turn number improved from 44 ms; docs/benchmarks.md +
  verification-snapshot updated to the fresh numbers.

## Round 23 — MCP server mode verified on the wire
- `pi --mcp-server` over stdio: proper `initialize` response (protocol
  2024-11-05, serverInfo "pi 0.1.0", tools capability) and a `tools/list`
  enumerating real tool schemas — the agent-as-MCP-server path works
  end-to-end offline.

## Round 24 — template feature verified (correct semantics)
- `--template` replaces the prompt (documented precedence); built-ins
  `{{root}}`/`{{model}}` and `--set key=value` substitutions all work:
  `Review this code: mycode (root=…, model=mock/echo)`. Early confusion was
  my misuse ({{input}} is a --set var, not a built-in) — the feature is
  correct as designed.

## Round 25 — headline numbers audited
- `--info` confirms the published 23 models / 15 tools exactly; the README's
  comparative performance claims (250–600× vs reference, --help ~1900×)
  remain the measured numbers since the comparison round. All documented
  counts are truthful to the binary.

## Round 26 — JSON mode audit
- All `--json` outputs verified parseable: `--sessions`, `--cost`, `--info`
  as documents; `--json --mode ask` as streaming JSONL (each line valid; my
  initial validator wrongly expected a single document). WSL remains offline.

## Round 27 — binaries current + full capstone at HEAD
- Static + release binaries verified newer than the last code change
  (currency confirmed). Full local gates at HEAD: 130 tests, clippy 0,
  sign-off 28/28, tour 12/12. CI green already confirmed remotely.

## Round 28 — installer now leaves profiles alone by default
- `install.sh` only touches `~/.profile` with EXPLICIT `UPDATE_PROFILE=1`
  (found it editing the host's profile during a local E2E — and reverted
  that edit immediately). Quiet-by-default installs can no longer surprise
  anyone's shell config. Verified both opt-in and default paths.

## Round 29 — verify matrix 28 -> 29 points
- Added the "installer quiet (no profile edits by default)" check to
  scripts/verify.sh (29 passed, 0 failed), and refreshed every current-state
  "28-point" reference to 29 (README, ARCHITECTURE, RELEASING, snapshot, CI +
  release workflows). The matrix grows as its invariants do.

## Round 30 — CI caught a portability bug in my new check; fixed
- The 29-point verify check (installer-quiet) broke CI: it required
  `dist/pi`, which CI runners never build. The badge exposed the failure.
  The check now stages `dist/pi` from the release binary (saving/restoring
  any pre-existing one) and passes both with and without it. Local 29/29.

## Round 31 — ONE-LINER ACCEPTANCE COMPLETE on real amd64
- On the provided Debian/WSL x86_64 box: run the release one-liner — the
  script (fetched via the GitHub API because the box's proxy serves a stale
  raw-file cache) downloaded the latest release, verified it WITHOUT
  `file(1)` (the fallback I wrote after the first attempt exposed the
  dependency), installed it, and the installed binary reports
  `x86_64 · e1a5849` (v1.0.3 commit). An offline agent turn through the
  installed binary ran to completion and wrote a session.
- The full story is proven: tag → CI green → release asset → one-line
  install → working pip binary on real hardware — including the two
  portability fixes (no file(1) dependency; quiet installer) it surfaced.

## Round 32 — serve E2E on real amd64 (final outer item closed)
- With the box finally available: shipped release binary `--serve` on the
  Debian/WSL x86_64 host — /healthz 200, an /rpc turn streamed events, and
  /sessions returned the created session. Every external protocol surface
  (install, agent loop, HTTP serve) is now verified on real amd64 hardware
  with the shipped binaries.

## Round 33 — the 29-point matrix RUNS on real amd64
- Cloned the repo to the Debian/WSL box, dropped the shipped static binary
  as ./pi, and ran scripts/verify.sh there: **28/28 applicable checks pass**
  on real x86_64 (the one skipped is the local-musl-gated "static binary
  runs" check). Serve, TUI (PTY decoder), the 12-step tour, and the
  installer-quiet invariant all verified against the shipped amd64 binary
  on real hardware. Round 33 in the restored goal.

## Round 34 — amd64 numbers confirmed current
- Re-measured the CURRENT release binary on the Debian/WSL box: --version
  best 2 ms, mock turn avg 6 ms — the published cross-arch numbers are
  accurate to the shipped artifact. Leftover CI-clone cleaned from the box.

## Round 35 — cross-arch memory profile completed
- serve RSS on real amd64 (current release): 3,372 kB, flat over 20 s — no
  leak on amd64 either. Together with the 2/6 ms benchmarks, the shipped
  binary's resource profile is now fully documented on both architectures.

## Round 36 — snapshot consolidated
- docs/verification-snapshot.md now records the definitive cross-arch
  acceptance: 29/29 local matrix, 28/28 applicable on real amd64, one-line
  install + serve E2E + benchmarks + RSS on the shipped binaries.

## Round 37 — v1.0.4 released via the 29-point pipeline
- Tagged v1.0.4: the release workflow ran the FULL 29-point sign-off + clippy
  + static-musl and published the newest release (latest = v1.0.4, verified
  ELF64 x86_64 static asset). The current matrix is now what every release
  gate runs.

## Round 38 — protocol matrix complete on both architectures
- On the Debian/WSL box with the shipped v1.0.4 binary: MCP server
  handshake (initialize → pi 0.1.0, tools/list = 15 tools) and `--review`
  (all three sections) verified. Combined with the earlier agent/serve/install
  checks, every external protocol surface is now verified on real amd64 AND
  the aarch64 host with the shipped binaries.

## Round 39 — final state audit
- Repo tree clean, ~/.profile clean (0 tmp refs), and the local `.pi`
  session litter from dogfooding removed (it's gitignored, so the repo was
  unaffected either way). Everything tidy ahead of the next work.

## Round 40 — snapshot labels current (v1.0.4 era)
- Clarified docs/verification-snapshot.md: title now v1.0.4 era and the
  release list includes v1.0.4. Every published reference is current.

## Round 41 — pi-mcp audited + hardened (the 5th crate reviewed)
- `pi-mcp` got the same OOM-hardening as the providers: bounded 1 MiB line
  reads and a 1 MiB tool-output cap with a truncation notice. A hostile-server
  regression test (a fake MCP server dumping a >1 MiB line) passes quickly
  with a bounded error. 131 tests, clippy 0; release + static rebuilt with
  the fix (dist refreshed).

## Round 42 — CI green with 131 tests; counts refreshed
- The pi-mcp hardening commit passed CI (badge: ci - passing). README,
  RELEASING and the verification snapshot now all say 131 tests / 131 / 131.

## Round 43 — v1.0.5 shipped
- Tagged v1.0.5: the release pipeline ran green and published the newest
  release with the pi-mcp-hardened static binary (verified ELF64 x86_64).
  Six releases now exist (v1.0.0–v1.0.5).

## Round 44 — v1.0.5 documented
- Snapshot updated to the v1.0.5 era; CHANGELOG gained the 1.0.5 entry
  (MCP bounded reads). The WSL box was offline for an on-hardware v1.0.5
  check (the same asset class was already accepted there in rounds 31–38).

## Round 45 — capstone at the 131-test state
- Full local gates: 131 tests, clippy 0, 29/29 sign-off, 12/12 tour,
  --version ~22 ms. The exact current commit is fully verified locally.

## Round 46 — external abort contract regression-checked
- `{"type":"abort"}` mid-turn now: the stream ended cleanly (done) at ~4 s
  with an 8 s sleep cut short and no tool-timeout involved. (First attempt
  hit a background-redirect race in MY test harness, not the server — the
  synchronous run proved the rpc stream intact.) 131 tests, clippy 0.

## Round 47 — usage.md flags reference added
- Discovered usage.md never documented ~20 real flags; appended the full
  current-binary flags reference (model overrides, serve controls, MCP/ext,
  export/gc/cost/tree/stats, templates/theme/skills, etc.). The doc now
  covers the complete surface.

## Round 48 — last stale counts swept
- ARCHITECTURE + CHANGELOG still said 130 tests; both now 131. Zero stale
  references remain anywhere in the docs/workflows.

## Round 49 — pi --doctor verified
- The current binary's environment diagnostics run cleanly: tool checks
  (git/sh/python3/node), cwd/repo/disk, terminal- and API-key advisories,
  pi.json/session/cost status — a tidy operator aid (rc=0 for a successful
  diagnostic).

## Round 50 — --install-package verified end-to-end
- A properly-formed package tar (entries under .pi/pi.json, skills/,
  templates/) installed cleanly: "installed 5 entries", all files landed in
  <root>/.pi, and the installed template rendered with {{root}}. The
  validator's rejection of my FIRST (malformed) package was correct security
  behavior — unsafe paths like `.`/.. are refused.

## Round 51 — verify matrix 29 -> 30 points
- The install-package flow (verified manually in round 50) is now a locked
  check: scripts/verify.sh runs a full package build→install→assert cycle
  (30 passed, 0 failed). All "29-point" references refreshed to 30.

## Round 52 — CI green with the 30-point matrix
- Badge: ci — passing for the commit carrying the package-install check;
  dist refreshed so the shipped verify.sh is the 30-point one.

## Round 53 — --tree verified on a forked session
- `pi --tree <session.jsonl>` renders the entry tree of a forked session:
  user → assistant → usage → forked turn, with ▶ head-branch markers and
  entry ids visible. The fork visualization works as documented.

## Round 54 — offline fallback testing infusion
- The scenario language gained a deliberate `error` rule
  ({"contains":…,"error":"…"}), the mock now streams errors from it, and —
  matching the real providers — the mock's stream wraps with the SAME
  fallback logic (consume-once: a fallback re-stream does not re-trip the
  error). A new binary-level test proves `--fallback` recovers offline while
  no-fallback runs fail loudly. 132 tests, clippy 0; release + static rebuilt.

## Round 55 — CI green with 132 tests; counts refreshed
- Badge: ci — passing for the fallback-infusion commit. User-facing docs now
  all say 132 tests.

## Round 56 — scenario error rule documented
- README's scenario section now documents the deliberate `error` rule and
  the offline `--fallback` exercise recipe.

## Round 57 — package-install tar-slip security fix
- Found in a self-review of packages.rs: the installer validated entry NAMES
  but not entry TYPES — a symlink entry (or writing THROUGH one) could
  escape <root> (tar-slip). The installer now refuses any non-regular/non-dir
  entry (symlink/special) via `tar -tvf` parsing, with a regression test
  crafting a poisoned package (`lrwxrwxrwx .pi/skills -> /etc` is rejected).
  Valid packages still install (30-point matrix passes). 133 tests, clippy 0;
  release + static rebuilt with the fix.

## Round 58 — v1.0.6 security release shipped
- The tar-slip fix released as v1.0.6 (latest): pipeline green, static asset
  verified ELF64 x86_64. Seven releases now (v1.0.0–v1.0.6); docs counts at
  133.

## Round 59 — tar-slip defense verified complete
- CI badge: ci — passing for the v1.0.6-era commit. Defense-in-depth check:
  hard links (`tar -tvf` → `hrw...`) are also rejected by the type parser,
  closing the symlink AND hardlink slip vectors for package install.

## Round 60 — security narrative current
- docs/comparison.md's hardening row now includes the pi-mcp bounds, offline
  fallback `error` rules, the package tar-slip fix, and the quiet installer,
  with the current 133/30/green state.

## Round 61 — CORS contract verified live
- `--serve --cors <origin>`: a browser-like OPTIONS preflight returns 204 +
  the configured Allow-Origin/Methods; a different origin receives NO CORS
  header (0 occurrences) — origin confinement holds in the current binary.

## Round 62 — serve-token auth contract verified live
- `--serve-token`: no token → 401, wrong token → 401, `Bearer <token>` →
  200, and /healthz stays open (200) — exactly the documented contract.

## Round 63 — /access-log framing bug found, fixed, check strengthened
- A live check exposed that `/access-log` (with `--cors` unset) ended HTTP
  headers right after the status line (the `\r\n{cc}\r\n` pattern double-
  terminated) — the real headers leaked into the body. The bug had shipped
  since the route existed because verify's check only grepped INSIDE the
  corrupted body. Fixed the format string; the check now also asserts the
  body starts clean. Also found a leaked pre-fix serve on port 8801 (from an
  earlier verification) that masked the fix — killed, plus a stray-process
  sweep. 30/30 including the strengthened check; static rebuilt with the fix.

## Round 64 — v1.0.7 bugfix release shipped
- The /access-log framing fix released as v1.0.7 (latest; pipeline green,
  asset verified ELF64 x86_64). Eight releases (v1.0.0–v1.0.7).

## Round 65 — adversarial HTTP sweep (current binary)
- 8 hostile requests (bad methods, path traversal, NUL, oversized
  content-length, empty/full rpc bodies, wildcard OPTIONS) — all handled
  cleanly, zero panics, server alive after. Combined with the framing fix,
  the serve surface is robust to the current state.

## Round 66 — release acceptance script (arch-aware)
- `scripts/acceptance.sh`: one command downloads the latest release, verifies
  ELF64 + static (no file(1) dependency), runs an offline agent turn +
  serve healthz on x86_64, and skips run checks cleanly on other arches
  (verified live on aarch64: 2 passed, 4 arch-skips). Documented in
  RELEASING.md; included in dist.

## Round 67 — acceptance.sh LOCAL_BIN path
- `LOCAL_BIN=<path>` runs the full acceptance against a local build on any
  arch. Verified: the aarch64 static binary passes 6/6 (build info, offline
  turn + session, serve healthz). Both acceptance paths (download + local)
  now work.

## Round 68 — snapshot/changelog at v1.0.7 era
- verification-snapshot.md era + release range refreshed; CHANGELOG gained
  the 1.0.6 (tar-slip) and 1.0.7 (framing) entries. Every published item
  now points at the newest state.

## Round 69 — CLI adversarial sweep
- 10 odd invocations (empty model/root, /dev/null session dir, non-numeric
  port, missing export file, bad completion shell, value-less --set,
  conflicting modes, no-session+resume, review without a repo) — zero panics,
  clean exit codes throughout.

## Round 70 — --export honest exit code
- From the CLI sweep: `--export <missing>` silently exported an empty
  session (rc=0). Now it fails loudly ("session file not found", rc=1);
  existing files still export (rc=0). Suite green at 133.

## Round 71 — rebuilt static accepted; export-all sanity
- Static (4,672,392 B) rebuilt with the export fix and accepted 6/6 (local
  path). --export-all behaves sanely on fresh and invalid target dirs. The
  WSL box remained offline (attempt after attempt; the acceptance script +
  earlier hardware runs cover it).

## Round 72 — HTML/txt export smoke verified
- `--export --format html` renders a complete standalone styled doc
  (doctype→</html>); txt renders the session header. Both CLI formats
  confirmed.

## Round 73 — skills verified
- `--skills rust` → "[skills] loaded 1 skill(s)" (system-prompt augmentation
  wired); a missing name → "warning: skills: skill 'nope' not found". The
  system isn't persisted to sessions by design — the loader messages confirm
  both paths.

## Round 74 — pi-to-pi MCP self-interop
- `pi --mcp-server` exposed 15 tools to a second `pi` running as an MCP
  client; a scenario-driver tool call round-tripped client → server → host
  bash → result ("[mcp] self: 15 tool(s)" + "tool result seen"). The MCP
  stack works against itself end to end.

## Round 75 — extension protocol (--ext) verified live
- A tiny ext server (`hello`→`ready` with a ping tool) attached via
  `--ext "demo python3 …"`; a scenario drove `ext:demo_ping` → "pong from
  ext" → loop closed ("ext result seen"). Both agent extension mechanisms
  (MCP in round 74, ext now) are verified with real subprocess servers.

## Round 76 — fs tools at scale
- 1 MB read_file and a 200-dir `**/f.txt` glob both ran without failure
  (content flows to the model per the documented paging model; no crash, no
  memory issue).

## Round 77 — dashboard HTML wiring verified
- The served dashboard page embeds the full client wiring: streaming rpc,
  the abort (stop button) path, /info and /sessions endpoints. The browser
  surface is complete.

## Round 78 — session ordering precision fix
- `mtime_s` was whole SECONDS — sessions written within the same second
  ordered arbitrarily, so `--resume` + `--gc` could mislabel files (the
  round-19 anomaly). Ordering now uses ms precision (`mtime_ms`); verified
  GC keeps a freshly resumed session and removes an older one. Suite at
  133, clippy 0; static rebuild pending.

## Round 79 — v1.0.8 released
- The ordering + export fixes shipped as v1.0.8 (pipeline green, static
  verified). Nine releases (v1.0.0–v1.0.8).

## Round 80 — --out root-confinement fix + 31-point matrix
- Found: `--out ../x` escaped `--root` (inconsistent with the file tools'
  `..` rejection). Now resolved through the shared `resolve_under` (rejected
  with a clear error). Locked as the 31st verify check; suite 133, clippy 0.

## Round 81 — v1.0.9 released (confinement fix)
- `--out` confinement shipped as v1.0.9 (pipeline green; static asset
  verified). CI badge still passing with the 31-point matrix. Ten releases.

## Round 82 — --file/--image confinement closed + matrix 32-point
- `--file ../x` and `--image ../x` could read/write OUTSIDE `--root` (same
  class as --out). Both now use the shared resolve_under (rejected with
  clear errors); the 32nd verify check locks it; an old test updated to the
  new semantics. 133 tests, clippy 0; release + static rebuilt with the fix.

## Round 83 — v1.0.10 released
- The --file/--image confinement fix shipped as v1.0.10 (pipeline green,
  asset verified). Eleven releases (v1.0.0–v1.0.10); CI badge passing.

## Round 84 — stdio RPC robustness sweep
- Malformed/partial frames (bad JSON, unknown type, empty, truncated) each
  produce a proper `{"type":"error"}` frame — zero panics; real prompts
  still complete with done/bye. The stdio protocol surface is robust.

## Round 85 — tool working-directory model verified + documented
- Confirmed empirically that every bash call starts in `--root` (cwd does
  not persist across calls — deterministic). Documented the model in
  docs/usage.md so agents/users navigate with explicit paths rather than
  relying on hidden state.

## Round 86 — periodic full capstone
- All gates in one pass at HEAD: 133 tests, clippy 0, sign-off 32/32,
  tour 12/12. Everything green; WSL box still offline.

## Round 87 — session forward-compat verified
- A synthetic foreign session (unknown event types, extra fields) loads
  without crashing through --stats, --export, and --resume (unknown entries
  skipped cleanly; a resumed turn completes). Older/newer session files
  won't break the current binary.

## Round 88 — theme spec honesty warning
- A malformed `--theme`/`$PI_THEME` silently fell back to default. The TUI
  now warns once when an explicit spec is neither inline JSON nor a theme
  file (verified in a real PTY session: warning shown for bad, silent for
  valid, no panics either way). Release rebuilt.

## Round 89 — v1.0.11 released
- Theme-honesty fix shipped as v1.0.11 (pipeline green, asset verified).
  Twelve releases (v1.0.0–v1.0.11).

## Round 90 — session forward-compat regression test
- Locked the round-87 finding: a real-entry line + an unknown-type line +
  garbage load without crashing, and the known entry is retained (using a
  serialized real Entry for the guaranteed shape). 134 tests, clippy 0.

## Round 91 — 134-test era confirmed
- Counts refreshed to 134; CI badge passing for the forward-compat commit.

## Round 92 — --init scaffold + shell completions verified
- `pi --root X --init` scaffolds pi.json + skills/ + sessions/; `--completion
  bash` (206 lines) and `fish` (66 lines) emit real completion scripts.

## Round 93 — --doctor strengthened
- --doctor now checks git identity (user.name/email) and runs a REAL offline
  mock turn to prove the whole local stack. Verified live: both new rows
  report ok, details accurate. 134 tests, clippy 0; static rebuilt.

## Round 94 — v1.0.12 released
- Doctor upgrade shipped (pipeline green, asset verified). Thirteen releases
  (v1.0.0–v1.0.12).

## Round 95 — repo completeness: LICENSE added, session litter purged
- Repo audit found NO LICENSE (added MIT) and two batches of tracked session
  litter (.pi at top level + custom-dir/nested) from early test runs — all
  purged, gitignored, and the CI hygiene guard extended to cover every
  session-data shape. 134 tests, clippy 0.

## Round 96 — RPC @-attachment asymmetry closed + ServeOpts refactor
- Found: the RPC prompt path (HTTP /rpc + stdio) did NOT attach in-line
  `@image.png` tokens — only the CLI path did (a real behavioral gap). The
  @-scanner is now a shared `inline_image_parts` helper used by both paths;
  the serve runtime gained a `ServeOpts` struct (8-arg lint went 0). Live
  checks: CLI + serve rpc both attach Base64 image parts; explicit `parts`
  still honored. 134 tests, clippy 0, verify 32/32, static rebuilt.

## Round 97 — v1.0.13 released
- RPC @-attachment fix shipped (pipeline green, asset verified). Fourteen
  releases (v1.0.0–v1.0.13).

## Round 98 — stdio RPC @-attachment fixed (full parity)
- The stdio RPC prompt path had the same gap as the HTTP one (round 96):
  it now builds parts via the shared inline_image_parts helper (+ explicit
  `parts` support). Live checks: @pic.png attaches Base64 in stdio rpc;
  explicit parts honored. ALL THREE prompt paths now behave identically.
  134 tests, clippy 0.

## Round 99 — v1.0.14 released
- Stdio RPC attachment fix shipped (pipeline green, asset verified).
  Fifteen releases (v1.0.0–v1.0.14).

## Round 100 — CHANGELOG up to date
- CHANGELOG gained the 1.0.8 → 1.0.14 entries (seven releases: ordering,
  export honesty, root confinement, doctor, theme honesty, RPC attachment
  parity). The record now matches the release history.

## Round 101 — --help flag drift check
- Scanned the in-binary --help against the documented flag set: all real
  flags present (serve-token, gc-dry-run, export-all, max-turns, login,
  install-package, mcp-server, …); the 4 scan "misses" were my scan's wrong
  names. No drift.

## Round 102 — binary self-containment check
- A renamed copy of the release binary, run from a foreign CWD, completes a
  full mock turn and writes its session — no hardcoded self-path or
  argv[0]/CWD dependence. Fully portable single ELF.

## Round 103 — media-type mapping verified
- `@photo.jpeg` attaches as `image/jpeg` (correct magic bytes encoded); the
  media_type_for mapping (png/jpeg/gif/webp) works end to end.

## Round 104 — --login key hygiene verified
- `--login` stores provider keys in `keys.json` with mode 0600; the schema
  (`provider` map) holds multiple providers, and keys load for the matching
  model/providers.

## Milestones
- [x] M0 scaffold + toolchain in-workspace
- [x] M1 pi-ai providers + catalog (mock/tested; real providers untested w/o keys)
- [x] M2 pi-agent loop + sessions + tools (mock-tested)
- [x] M3 pi-tui driver + diff renderer + widgets
- [x] M4 pi binary print mode end-to-end
- [x] M5 interactive TUI + slash commands (**PTY-validated** this round)
- [~] M6 extensions **done**, themes **done**, skills **done**, templates **done**, config **done**
- [x] M7 MCP client **done**; session resume/fork/--continue/list/gc/**tree** **done**; /replay in TUI **done**
- [~] M8 parallel tools + bg bash + todos + fs + apply_diff + images **done**; diff view → git/review
- [~] M9 hardening: bench + clippy + fuzz **done**, **musl static build done**; docs done (README)
- [x] M10 model routing (fallback + config), cost dashboard, /review+git, pi serve, stats, /replay **done**; **beyond: --mcp-server, templates, skills, themes, doctor, --file/--out, --image, gc, tree, scenario tests**
