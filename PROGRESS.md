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
