# pi usage tour

`pi` is a terminal coding agent: a model, a file-sandboxed toolset, durable
sessions, and a TUI. Everything below also works against the `mock/echo`
provider or a fake OpenAI-compatible endpoint, so you can try every command
with no API key.

## Modes

```
pi                              # interactive TUI (on a TTY)
pi "review this repo and propose fixes"   # one-shot print mode
pi -i "my first task"           # open the TUI, send the prompt as msg #1
echo "explain this file" | pi   # prompt from stdin
pi --mode ask "what is a monad?"         # no tools, conversational
pi --json "run the tests"                # NDJSON events to stdout
```

## Model selection

```
pi -m openai/gpt-4o "…"
pi -m anthropic/claude-sonnet-4-5 "…"
pi -m custom:http://localhost:1234/v1:my-model "…"   # any OpenAI-compat server
pi --fallback anthropic/claude-haiku "…"             # auto-fallback model
pi --list-models --json                              # 23 built-in models
pi --info                                            # build banner
```

Flags may appear before or after the prompt; use `--` before a prompt that
starts with a dash.

## Sessions

Every message persists as JSONL under `<root>/.pi/sessions/`.

```
pi --sessions                          # list (labels, counts, sizes)
pi --sessions --json                   # machine-readable
pi --stats .pi/sessions/<id>.jsonl     # one session in detail
pi --stats .pi/sessions/<id>.jsonl --json
pi --tree .pi/sessions/<id>.jsonl      # fork-aware entry tree (▶ = head)
pi --continue "keep going"             # resume the newest session
pi --resume <id> "second attempt"      # resume a specific session
pi --gc-sessions 20                    # prune old sessions
pi --grep-sessions "parsing bug"          # search all sessions
pi --cost                              # aggregate tokens/cost across sessions
pi --export .pi/sessions/<id>.jsonl            # markdown
pi --export .pi/sessions/<id>.jsonl --format html|txt|json
pi --no-session "…"                    # stateless run
```

In the TUI: `/label my-task` names the current session, `/fork <n>` rewinds
and branches, `/replay <n>` shows an entry, `/new` starts fresh.

## Tools (file tools sandboxed to `--root`; **bash is host-level** by design)

`read/write/edit/apply_diff/glob/grep/fs/git` are confined to `--root`
(absolute paths, `..`, and escaping symlinks are rejected). `bash` runs
`sh -c` on the host — a coding agent needs the shell — so run pi with
`--root` set to the project directory you intend the agent to affect.

read_file · write_file · edit_file · glob · grep · list_dir · bash (timeout
with `timeout_ms` override, group-kill) · todo (plan list) · git (read-only
whitelist) · bg_bash/bg_read/bg_kill/bg_list (long-running jobs) · fs
(list/stat/du) · apply_diff (unified diffs, git-equivalent).

`--tool-timeout <ms>` sets the global bash default.

## Agent loop behavior

- **`--plan` / `-P`:** the agent creates a todo list and works through it.
- **Compaction:** auto-compacts past `compact_threshold` of the context
  window; `/compact` forces it.
- **`--max-output <n>`:** cap tokens per turn.
- **`--fallback <ref>`:** on provider error, retry with the backup model.
- **Parallel tool calls:** multiple tool calls in one response run
  concurrently.

## Automation interfaces

```
# stdio JSON-lines RPC
printf '{"id":1,"type":"prompt","input":"hi"}\n{"type":"quit"}\n' | pi --rpc

# MCP server: pi exposes its whole toolset to any MCP client
echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | pi --mcp-server

# HTTP server
pi --serve --port 8787
curl http://127.0.0.1:8787/healthz
curl http://127.0.0.1:8787/info
curl http://127.0.0.1:8787/metrics
printf '{"id":1,"type":"prompt","input":"refactor main.rs"}' | \
  curl -sN --data-binary @- http://127.0.0.1:8787/rpc   # NDJSON stream
pi --serve --serve-token s3cret        # bearer-auth everything but healthz
pi --serve --access-log                 # GET /access-log (last 200 requests)
# open http://localhost:8787/ in a browser: embedded dashboard that
# streams agent turns live over POST /rpc (note: the dashboard is meant for
# --serve without --serve-token; with a token use a real HTTP client)

# async background tasks / tool plugins
pi --ext "<name> <command> [args...]" "…"        # subprocess plugin (JSON-lines protocol)
pi --mcp "<name> <command> [args...]" "…"        # MCP client (own tools become ext:<name>_<tool>)

# offline, deterministic agent runs (tests/documentation)
PI_MOCK_SCENARIO=scn.jsonl pi "…"                 # scripted tool loop
```

## Review & config

```
pi --review                 # git status + diff -> model review
pi --doctor --json          # environment diagnostics
pi --init                   # scaffold <root>/.pi (pi.json, skills/, sessions/)
# <root>/.pi/pi.json: model, system, temperature, max_turns, thinking,
#   skills, theme, compact_threshold, session_dir
pi --template plan --set goal="x" "…"  # templates render with {{vars}}
pi --theme one-dark         # or inline JSON / $PI_THEME
pi --login openai=sk-...     # store a provider key (.pi/keys.json, mode 0600)
pi --logout openai           # remove a stored key
```

## Shell integration

```
pi --completion bash  | source /dev/stdin   # bash/zsh/fish completions
scripts/package.sh <root> <out.tar.gz>        # bundle .pi/ config
pi --install-package out.tar.gz               # install it (safe untar)
pi --man > pi.1 && man ./pi.1              # roff man page
```

## Flags reference (current binary)

| flag | meaning |
|---|---|
| `-m, --model` | model ref (`mock/echo`, `anthropic/...`, `custom:<base>:<model>`); default mock or pi.json |
| `--root` | working dir that file tools are confined to (default cwd) |
| `--system` | extra system-prompt text |
| `--api-key` / `--base-url` | provider key / OpenAI-compatible base URL overrides |
| `--json` | stream agent events as JSON lines (print mode) |
| `-i, --interactive` | force the TUI |
| `--no-tools` | disable all built-in tools |
| `--max-turns` / `--temperature` / `--thinking` | turn cap / sampling / reasoning effort |
| `--rpc` | JSON-RPC over stdio |
| `--list-models` | list resolvable models |
| `--resume <id>` / `--continue` | resume / continue latest session |
| `--quiet` | hide tool trace in print mode |
| `--session-dir` | sessions directory |
| `--mcp "<name> <cmd>"` | attach an MCP server's tools (repeatable) |
| `--ext "<name> <cmd>"` | attach an extension process (repeatable) |
| `--review` | feed git status + diff to the model as the prompt |
| `--serve` / `--port` / `--serve-addr` / `--serve-token` / `--cors` / `--access-log` | HTTP JSON-RPC server controls |
| `--completion <shell>` / `--man` | shell completion / roff man page |
| `--install-package <tar>` | install a pi package into `<root>/.pi/` |
| `--export-all <dir>` | batch-export every session as `<id>.md` |
| `--grep-sessions <text>` | search session files |
| `--cost` | aggregate usage/cost across sessions |
| `--login p=key` / `--logout p` | manage stored provider keys (mode 0600) |
| `--info` | build info (version, counts, target, build sha) |
| `--theme <spec>` | TUI color theme (inline JSON or file; `$PI_THEME`) |
| `--fallback <ref>` | model fallback when the primary fails |
| `--skills <names>` / `--skills-dir` | load skills into the system prompt |
| `--doctor` | environment diagnostics |
| `--stats <file>` / `--tree <file>` | per-session stats / fork tree |
| `--image <file>` | attach an image (repeatable) |
| `--init` | scaffold `<root>/.pi/` |
| `--export <file>` / `--format` | export a session (md/txt/html) |
| `--mode code\|ask` | tools enabled / conversational |
| `--file <f>` / `--out <f>` | prompt from file / answer to file |
| `--sessions` / `--gc-sessions <n>` / `--gc-dry-run` | list / prune / preview sessions |
| `--template <name>` / `--set k=v` | prompt templates with variables |
| `--no-session` | stateless run |
| `--mcp-server` | serve pi's tools as an MCP server over stdio |
| `--tool-timeout <ms>` | default bash timeout |
| `--max-output <tokens>` | per-turn output cap |

Regenerate anytime with `pi --help` (and `pi --man` for the roff page).
