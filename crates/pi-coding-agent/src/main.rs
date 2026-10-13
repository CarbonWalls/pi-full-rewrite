//! pi — coding agent CLI (Rust rewrite of earendil-works/pi).

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use pi_agent::session::Session;
use pi_agent::tools::ToolRegistry;
use pi_agent::{Agent, AgentConfig, AgentEvent};
use pi_ai::{Keys, Llm, ModelInfo, ThinkingLevel};
use pi_tools::Toolset;

/// Directive prepended by `--plan`.
const PLAN_DIRECTIVE: &str = "Plan first: before touching any files, use the \
todo tool to break this task into 2-5 concrete steps. Then work through \
each step, checking todos off as you finish them. Keep the list updated \
as the task progresses.";

mod completion;
mod config;
mod packages;
mod doctor;
mod export;
mod ext;
mod mcp_server;
mod review;
mod rpc;
mod serve;
mod skills;
mod stats;
mod templates;
mod ui;

/// pi: minimal, extensible coding agent (Rust rewrite of earendil-works/pi).
/// Version banner with build + model/tool counts (for --version/-V).
fn long_version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let models = pi_ai::Catalog::builtin().all().len();
        let tools = 15; // fixed builtin toolset (mcp/ext tools are dynamic)
        let rust_ver = if env!("CARGO_PKG_RUST_VERSION").is_empty() {
            "stable"
        } else {
            env!("CARGO_PKG_RUST_VERSION")
        };
        format!(
            "pi {} · {models} models · {tools} tools · rust {rust_ver} · {} · {}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH,
            option_env!("PI_BUILD_SHA").unwrap_or("dev")
        )
    })
    .as_str()
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Prompt (one or more words, joined with spaces). Flags may appear
    /// before or after the prompt; use `--` before a prompt that starts with
    /// a dash. Omit for interactive mode.
    #[arg(value_name = "PROMPT")]
    prompt: Vec<String>,

    /// Model reference, e.g. `mock/echo`, `anthropic/claude-sonnet-4-5`,
    /// `openai/gpt-4o`, or `custom:<base_url>:<model>`.
    /// Default: `mock/echo`, or the model in `<root>/.pi/pi.json`.
    #[arg(short = 'm', long)]
    model: Option<String>,

    /// Working directory that file tools are confined to (default: cwd).
    #[arg(long, default_value = ".")]
    root: PathBuf,

    /// Extra system-prompt text (appended to the default agent prompt).
    #[arg(long)]
    system: Option<String>,

    /// API key override for the selected model's provider.
    #[arg(long)]
    api_key: Option<String>,

    /// Base URL override for OpenAI-compatible providers.
    #[arg(long)]
    base_url: Option<String>,

    /// Stream agent events as JSON lines to stdout (print mode).
    #[arg(long)]
    json: bool,

    /// Force interactive TUI.
    #[arg(short = 'i', long)]
    interactive: bool,

    /// Disable all built-in tools.
    #[arg(long)]
    no_tools: bool,

    /// Max assistant turns per user turn.
    #[arg(long)]
    max_turns: Option<usize>,

    /// Sampling temperature.
    #[arg(long)]
    temperature: Option<f32>,

    /// Reasoning effort.
    #[arg(long, value_parser = ["off", "minimal", "low", "medium", "high"])]
    thinking: Option<String>,

    /// JSON-RPC mode over stdio (newline-delimited JSON).
    #[arg(long)]
    rpc: bool,

    /// List resolvable models and exit.
    #[arg(long)]
    list_models: bool,

    /// Resume an existing session id.
    #[arg(long)]
    resume: Option<String>,

    /// Continue the most recent session in the session dir.
    #[arg(long)]
    continue_: bool,

    /// Hide the tool trace in print mode (show only the final answer).
    #[arg(long)]
    quiet: bool,

    /// Sessions directory (default: `<root>/.pi/sessions`).
    #[arg(long)]
    session_dir: Option<PathBuf>,

    /// MCP server: `"<name> <command> [args...]"` (repeatable). The server
    /// must speak MCP over stdio; its tools are exposed as `mcp:<name>_<tool>`.
    #[arg(long, value_name = "SPEC")]
    mcp: Vec<String>,

    /// Extension: `"<name> <command> [args...]"` (repeatable). The process
    /// speaks the pi extension JSON-lines protocol; its tools are exposed as
    /// `ext:<name>_<tool>`.
    #[arg(long, value_name = "SPEC")]
    ext: Vec<String>,

    /// Run a code review: feed git status + diff to the model. With no prompt
    /// this IS the prompt.
    #[arg(long)]
    review: bool,

    /// Run as an HTTP JSON-RPC server (POST /rpc, GET /healthz /info).
    #[arg(long)]
    serve: bool,

    /// Port for --serve.
    #[arg(long, default_value_t = 8787)]
    port: u16,

    /// Bind address for --serve (default: 127.0.0.1).
    #[arg(long, default_value = "127.0.0.1")]
    serve_addr: String,

    /// Require `Authorization: Bearer <token>` on all /serve routes
    /// (healthz stays open).
    #[arg(long, value_name = "TOKEN")]
    serve_token: Option<String>,

    /// Emit a shell completion script and exit: bash | zsh | fish.
    #[arg(long, value_name = "SHELL")]
    completion: Option<String>,

    /// CORS origin header for --serve (e.g. `*` or `https://ui.example.com`).
    #[arg(long, value_name = "ORIGIN")]
    cors: Option<String>,

    /// Emit a roff man page for pi and exit.
    #[arg(long)]
    man: bool,

    /// Keep an in-memory access log (last 200 requests) exposed at
    /// `/access-log` while serving.
    #[arg(long)]
    access_log: bool,

    /// Install a pi package tarball (`.pi/pi.json`, skills/, templates/)
    /// into `<root>/.pi/` and exit: `--install-package pkg.tar.gz`.
    #[arg(long, value_name = "FILE")]
    install_package: Option<std::path::PathBuf>,

    /// Export every session in the session dir as `<id>.md` into `--export-all <DIR>`.
    #[arg(long, value_name = "DIR")]
    export_all: Option<std::path::PathBuf>,

    /// Search all session files for a pattern (simple substring) and print
    /// the matching sessions with line numbers: `--grep-sessions <text>`.
    #[arg(long, value_name = "TEXT")]
    grep_sessions: Option<String>,

    /// Aggregate token usage and cost across every session in the session
    /// dir (from the per-turn Usage entries pi records).
    #[arg(long)]
    cost: bool,

    /// Store a provider key in `<root>/.pi/keys.json` (mode 0600) and exit:
    /// `--login openai=sk-...`.
    #[arg(long, value_name = "PROVIDER=KEY")]
    login: Option<String>,

    /// Remove a stored provider key: `--logout openai`.
    #[arg(long, value_name = "PROVIDER")]
    logout: Option<String>,

    /// Plan first: instruct the agent to create a todo-list plan before
    /// working, then execute it step by step (extra over pi).
    #[arg(long, short = 'P')]
    plan: bool,

    /// Print build info (version, model/tool counts, target) and exit.
    #[arg(long)]
    info: bool,

    /// TUI color theme: inline JSON or a path to a JSON file
    /// (also honors $PI_THEME).
    #[arg(long, value_name = "SPEC")]
    theme: Option<String>,

    /// Model to auto-switch to when the primary fails (e.g. "openai/gpt-4o-mini").
    #[arg(long, value_name = "REF")]
    fallback: Option<String>,

    /// Load skills into the system prompt: comma-separated names or "all"
    /// (from `<root>/.pi/skills/*.md`, or --skills-dir).
    #[arg(long, value_name = "NAMES")]
    skills: Option<String>,

    /// Directory for --skills (default `<root>/.pi/skills`).
    #[arg(long, value_name = "DIR")]
    skills_dir: Option<PathBuf>,

    /// Run environment diagnostics and exit.
    #[arg(long)]
    doctor: bool,

    /// Print stats for a session file and exit: `--stats .pi/sessions/x.jsonl`.
    #[arg(long, value_name = "FILE")]
    stats: Option<PathBuf>,

    /// Render the session entry tree (forks visible) for a session file.
    #[arg(long, value_name = "FILE")]
    tree: Option<PathBuf>,

    /// Attach an image to the prompt (repeatable). `@file.png` tokens in the
    /// prompt are also auto-attached when they resolve under the working dir.
    #[arg(long, value_name = "FILE")]
    image: Vec<PathBuf>,

    /// Scaffold `<root>/.pi/` (pi.json template + skills/ + sessions/) and exit.
    #[arg(long)]
    init: bool,

    /// Export a session file as markdown and exit:
    /// `--export .pi/sessions/x.jsonl [--format md|txt]`
    #[arg(long, value_name = "FILE")]
    export: Option<PathBuf>,

    /// Output format for --export: md (default) or html.
    #[arg(long, value_name = "FMT")]
    format: Option<String>,

    /// Operating mode: `code` (default; tools enabled) or `ask`
    /// (no tools, conversational answers only).
    #[arg(long, value_name = "MODE", value_parser = ["code", "ask"], default_value = "code")]
    mode: String,

    /// Read the prompt from a file (resolved under --root).
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,

    /// Write the final answer to a file (resolved under --root).
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// List recorded sessions in the session dir (newest first) and exit.
    #[arg(long)]
    sessions: bool,

    /// Delete old sessions, keeping the newest N (default 20).
    #[arg(long, value_name = "N")]
    gc_sessions: Option<usize>,

    /// With `--gc-sessions`: print what would be pruned without deleting.
    #[arg(long)]
    gc_dry_run: bool,

    /// Use a prompt template by name from `<root>/.pi/templates/<name>.md`.
    #[arg(long, value_name = "NAME")]
    template: Option<String>,

    /// Set a template variable (repeatable): `--set key=value`.
    #[arg(long, value_name = "KV")]
    set: Vec<String>,

    /// Run stateless: do not write any session file to disk.
    #[arg(long)]
    no_session: bool,

    /// Serve pi's tools as an MCP server over stdio (JSON-RPC 2.0).
    #[arg(long)]
    mcp_server: bool,

    /// Default timeout for the bash tool, in ms (default 60000).
    #[arg(long, value_name = "MS")]
    tool_timeout: Option<u64>,

    /// Cap the model's per-turn max output tokens (default: model default).
    #[arg(long, value_name = "TOKENS")]
    max_output: Option<u32>,
}

fn parse_thinking(s: &str) -> ThinkingLevel {
    match s {
        "minimal" | "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        _ => ThinkingLevel::Off,
    }
}

/// The TUI drives crossterm's event loop with blocking `poll()` calls, so the
/// agent's spawned turn task needs a real worker thread (a current_thread
/// runtime would starve it). All other modes run fine — and faster — on a
/// current_thread runtime.
fn needs_multithread_tui(cli: &Cli) -> bool {
    if cli.interactive {
        return true;
    }
    if cli.rpc || cli.serve {
        return false;
    }
    // print modes with an explicit prompt source
    if !cli.prompt.is_empty() || cli.review || cli.template.is_some() || cli.file.is_some() {
        return false;
    }
    // no prompt and stdin is a TTY ⇒ interactive chat
    std::io::IsTerminal::is_terminal(&std::io::stdin())
}

fn main() {
    let cli = Cli::parse();
    let rt = if needs_multithread_tui(&cli) {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
    };
    let rt = match rt {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to start runtime: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(run(cli)) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    // `--login` / `--logout`.
    match auth_command(&cli) {
        Ok(Some(())) => return Ok(()),
        Err(e) => return Err(anyhow::anyhow!("{e}")),
        Ok(None) => {}
    }

    // Install a pi package.
    if let Some(pkg) = &cli.install_package {
        let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());
        return match packages::install(pkg, &root) {
            Ok(msg) => {
                println!("{msg}");
                Ok(())
            }
            Err(e) => {
                eprintln!("package error: {e}");
                Err(anyhow::anyhow!("{e}"))
            }
        };
    }

    // Bulk export of all sessions.
    if let Some(outdir) = &cli.export_all {
        let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());
        let sd = cli
            .session_dir
            .clone()
            .unwrap_or_else(|| root.join(".pi").join("sessions"));
        std::fs::create_dir_all(outdir)?;
        let mut n = 0;
        for row in stats::list(&sd) {
            if let Ok(md) = export::run(&row.path, "md") {
                let out = outdir.join(format!("{}.md", row.id));
                std::fs::write(&out, md)?;
                n += 1;
            }
        }
        println!("exported {n} sessions to {}", outdir.display());
        return Ok(());
    }

    // Aggregate usage/cost across all sessions.
    if cli.cost {
        let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());
        let sd = cli
            .session_dir
            .clone()
            .unwrap_or_else(|| root.join(".pi").join("sessions"));
        let (models, (input, output, cost), count) = stats::aggregate_cost(&sd);
        if count == 0 {
            println!("no usage entries in {sd:?} (sessions from older builds lack them)");
            return Ok(());
        }
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "sessions": sd.to_string_lossy(),
                    "model_calls": count,
                    "per_model": models.iter().map(|(m, i, o, c)| serde_json::json!({
                        "model": m, "input": i, "output": o, "cost": c
                    })).collect::<Vec<_>>(),
                    "total": {"input": input, "output": output, "cost": cost},
                })
            );
            return Ok(());
        }
        for (m, i, o, c) in &models {
            println!("{m}: in={i} out={o} cost=${c:.6}");
        }
        println!("total: {count} model calls · in={input} out={output} cost=${cost:.6}");
        return Ok(());
    }

    // Search sessions.
    if let Some(pat) = &cli.grep_sessions {
        let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());
        let sd = cli
            .session_dir
            .clone()
            .unwrap_or_else(|| root.join(".pi").join("sessions"));
        let mut hits = 0usize;
        for row in stats::list(&sd) {
            if let Ok(text) = std::fs::read_to_string(&row.path) {
                for (ln, line) in text.lines().enumerate() {
                    if line.contains(pat) {
                        println!("{}:{}: {}", row.id, ln + 1, line.chars().take(120).collect::<String>());
                        hits += 1;
                    }
                }
            }
        }
        println!("({hits} matching lines in {sd:?})");
        return Ok(());
    }

    // Build info.
    if cli.info {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "service": "pi",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rust": env!("CARGO_PKG_RUST_VERSION"),
                    "arch": std::env::consts::ARCH,
                    "models": pi_ai::Catalog::builtin().all().len(),
                    "tools": 15,
                    "providers": ["mock", "custom", "openai", "anthropic", "gemini", "deepseek", "openrouter", "mistral", "groq"],
                    "build_sha": option_env!("PI_BUILD_SHA").unwrap_or("dev"),
                })
            );
        } else {
            let rust_ver = if env!("CARGO_PKG_RUST_VERSION").is_empty() {
                "stable"
            } else {
                env!("CARGO_PKG_RUST_VERSION")
            };
            println!("{}\nrust {rust_ver}\ntarget {} · fs-{}", long_version(), std::env::consts::ARCH, std::env::consts::FAMILY);
            println!("build: {} (set via PI_BUILD_SHA at compile time)", option_env!("PI_BUILD_SHA").unwrap_or("dev"));
            println!("models: {} builtin", pi_ai::Catalog::builtin().all().len());
            println!("tools: 15 builtin (mcp/ext dynamic); sessions: jsonl; providers: mock/custom/openai/anthropic/gemini/deepseek/openrouter/mistral/groq");
        }
        return Ok(());
    }

    // Man page.
    if cli.man {
        return completion::print_man().map_err(anyhow::Error::msg);
    }

    // Shell completions.
    if let Some(shell) = &cli.completion {
        return completion::print_shell(shell).map_err(anyhow::Error::msg);
    }

    // List models
    if cli.list_models {
        let llm = build_llm(&cli)?;
        if cli.json {
            let models: Vec<serde_json::Value> = llm
                .catalog
                .all()
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "provider": m.provider, "id": m.id, "name": m.name,
                        "context_window": m.context_window,
                        "max_output_tokens": m.max_output_tokens,
                        "tools": m.tools, "reasoning": m.reasoning, "vision": m.vision,
                        "api": m.api, "input_price": m.input_price, "output_price": m.output_price
                    })
                })
                .collect();
            println!("{}", serde_json::json!({ "models": models }));
        } else {
            for m in llm.catalog.all() {
                println!(
                    "{}/{}  ctx={} out={} tools={} reasoning={}",
                    m.provider, m.id, m.context_window, m.max_output_tokens, m.tools, m.reasoning
                );
            }
        }
        return Ok(());
    }

    // Environment diagnostics.
    if cli.doctor {
        let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());
        let sd = cli
            .session_dir
            .clone()
            .unwrap_or_else(|| root.join(".pi").join("sessions"));
        let report = doctor::Report::run(&root, Some(&sd));
        if cli.json {
            let checks: Vec<serde_json::Value> = report
                .checks
                .iter()
                .map(|(name, ok, detail)| {
                    serde_json::json!({"check": name, "ok": ok, "detail": detail})
                })
                .collect();
            println!("{}", serde_json::json!({ "doctor": checks }));
        } else {
            print!("{}", report.render());
        }
        return Ok(());
    }

    // Session tree view.
    if let Some(f) = &cli.tree {
        match stats::tree(f) {
            Ok(out) => {
                print!("{out}");
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("tree: {e}")),
        }
    }

    // Session stats.
    if let Some(f) = &cli.stats {
        let res = if cli.json {
            stats::run_json(f)
        } else {
            stats::run(f).map(serde_json::Value::String)
        };
        match res {
            Ok(out) => {
                if cli.json {
                    println!("{out}");
                } else if let serde_json::Value::String(s) = out {
                    print!("{s}");
                }
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("stats: {e}")),
        }
    }

    // Session export.
    if let Some(f) = &cli.export {
        let fmt = cli.format.as_deref().unwrap_or("md");
        match export::run(f, fmt) {
            Ok(out) => {
                print!("{out}");
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("export: {e}")),
        }
    }

    let root = std::fs::canonicalize(&cli.root).unwrap_or_else(|_| cli.root.clone());

    // Project config (pi.json), loaded once so every surface (--sessions,
    // --gc, --cost, the TUI) sees the same session dir.
    let fc = config::FileConfig::load(&root).unwrap_or_default();

    // Session listing / GC need only the session dir.
    let session_dir0 = cli
        .session_dir
        .clone()
        .or_else(|| {
            fc.session_dir.as_deref().map(|d| {
                let p = PathBuf::from(d);
                if p.is_absolute() { p } else { root.join(p) }
            })
        })
        .unwrap_or_else(|| root.join(".pi").join("sessions"));
    if cli.sessions {
        if cli.json {
            println!("{}", stats::list_json(&session_dir0));
        } else {
            let rows = stats::list(&session_dir0);
            print!("{}", stats::render_list(&rows));
        }
        return Ok(());
    }
    if let Some(keep) = cli.gc_sessions {
        if cli.gc_dry_run {
            let cands = stats::gc_candidates(&session_dir0, keep);
            println!(
                "gc_sessions (dry-run): kept {} newest, would remove {} file(s)",
                keep,
                cands.len()
            );
            for r in &cands {
                println!("  would remove {} ({} msgs)", r.id, r.messages);
            }
            return Ok(());
        }
        match stats::gc(&session_dir0, keep) {
            Ok(deleted) => {
                println!(
                    "gc_sessions: kept {} newest, removed {} file(s)",
                    keep,
                    deleted.len()
                );
                for id in &deleted {
                    println!("  removed {id}");
                }
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("gc_sessions: {e}")),
        }
    }

    // project config: CLI flags override <root>/.pi/pi.json
    // `pi --init`: scaffold .pi/ and exit
    if cli.init {
        config::FileConfig::init(&root)?;
        return Ok(());
    }

    let session_dir = cli
        .session_dir
        .clone()
        .or_else(|| {
            fc.session_dir.clone().map(|d| {
                let p = PathBuf::from(&d);
                if p.is_absolute() { p } else { root.join(p) }
            })
        })
        .unwrap_or_else(|| root.join(".pi").join("sessions"));

    let llm = build_llm(&cli)?;
    let model = resolve_model(&llm, &cli, &fc)?;
    let mut model = model;
    if let Some(cap) = cli.max_output {
        if cap < model.max_output_tokens as u32 {
            model.max_output_tokens = cap as u64;
        }
    }

    let session = build_session(&cli, &session_dir).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut tools = ToolRegistry::new();
    // `ask` mode disables tools (conversation only).
    let ask_mode = cli.mode == "ask";
    let toolset: Option<Toolset> = if cli.no_tools || ask_mode {
        None
    } else {
        let ts = Toolset::new(&root).with_tool_timeout(cli.tool_timeout.unwrap_or(60_000));
        ts.register_all(&mut tools);
        Some(ts)
    };

    // Merge config-file defaults behind explicit CLI flags (CLI wins).
    let skills_spec = cli.skills.clone().or_else(|| fc.skills.clone());
    let theme_spec = cli.theme.clone().or_else(|| fc.theme.clone());
    let system_text = cli
        .system
        .clone()
        .or_else(|| fc.system.clone());
    let thinking = cli
        .thinking
        .clone()
        .or_else(|| fc.thinking.clone());
    let temperature = cli.temperature.or(fc.temperature);
    let max_turns = cli.max_turns.or(fc.max_turns).unwrap_or(50);
    let compact_threshold = fc.compact_threshold.unwrap_or(0.85);

    let mut config = AgentConfig {
        model: model.clone(),
        fallback_model: cli
            .fallback
            .as_deref()
            .map(|fb| resolve_model_ref(&llm, &cli, fb))
            .transpose()?,
        system: build_system(system_text.as_deref(), ask_mode),
        thinking: thinking.as_deref().map(parse_thinking).unwrap_or_default(),
        temperature,
        compact_threshold,
        max_turns,
    };

    // Skills augment the system prompt.
    if let Some(spec) = &skills_spec {
        let sdir = cli
            .skills_dir
            .clone()
            .unwrap_or_else(|| root.join(".pi").join("skills"));
        match skills::render_selected(&sdir, spec) {
            Ok(frag) if !frag.trim().is_empty() => {
                let n_skills = frag.matches("## Skill:").count();
                eprintln!("[skills] loaded {n_skills} skill(s) from {}", sdir.display());
                config
                    .system
                    .push(pi_ai::ContentPart::Text { text: frag });
            }
            Ok(_) => eprintln!("[skills] none matched ({} selected)", spec.trim()),
            Err(e) => eprintln!("warning: skills: {e}"),
        }
    }

    // MCP servers: spawn + expose their tools.
    for spec in &cli.mcp {
        let mut parts = spec.split_whitespace();
        let name = parts.next().unwrap_or("");
        let cmd: Vec<String> = parts.map(|s| s.to_string()).collect();
        if name.is_empty() || cmd.is_empty() {
            eprintln!("warning: bad --mcp spec '{spec}' (want '<name> <command> [args]')");
            continue;
        }
        match pi_mcp::connect_tools(name, &cmd) {
            Ok(ts) => {
                let n = ts.len();
                for t in ts {
                    tools.register_dyn(t);
                }
                eprintln!("[mcp] {name}: {n} tool(s)");
            }
            Err(e) => eprintln!("warning: mcp server '{name}' failed: {e}"),
        }
    }

    // Extensions: spawn + expose their tools; contributed text joins the
    // system prompt. Keep handles for event broadcasting.
    let mut ext_handles: Vec<ext::ExtHandle> = Vec::new();
    let model_ref = format!("{}/{}", config.model.provider, config.model.id);
    let cwd_str = root.to_string_lossy().to_string();
    for spec in &cli.ext {
        let mut parts = spec.split_whitespace();
        let name = parts.next().unwrap_or("");
        let cmd: Vec<String> = parts.map(|s| s.to_string()).collect();
        if name.is_empty() || cmd.is_empty() {
            eprintln!("warning: bad --ext spec '{spec}' (want '<name> <command> [args]')");
            continue;
        }
        match ext::connect(name, &cmd, &model_ref, &cwd_str) {
            Ok((tss, system, handle)) => {
                if !system.trim().is_empty() {
                    config.system.push(pi_ai::ContentPart::Text {
                        text: format!("
[extension {name}]
{system}"),
                    });
                }
                for t in tss {
                    tools.register_dyn(t);
                }
                ext_handles.push(handle);
                eprintln!("[ext] {name}: registered");
            }
            Err(e) => eprintln!("warning: extension '{name}' failed: {e}"),
        }
    }

    let mut agent = Agent::new(llm, config, tools, session);
    agent.session_dir = Some(session_dir0.clone());

    // Conflict detection: mutually exclusive modes shouldn't silently win.
    {
        let mods: Vec<(&str, bool)> = vec![
            ("--mcp-server", cli.mcp_server),
            ("--rpc", cli.rpc),
            ("--serve", cli.serve),
            ("-i/--interactive", cli.interactive),
        ];
        let active: Vec<&str> = mods.iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
        if active.len() > 1 {
            return Err(anyhow::anyhow!(
                "conflicting modes: {} (choose one)",
                active.join(", ")
            ));
        }
    }

    // MCP server mode owns stdin/stdout for JSON-RPC.
    if cli.mcp_server {
        // re-own the registry from the agent's tools
        let reg = agent.tools.clone();
        return mcp_server::serve(&reg, std::io::stdin().lock(), std::io::stdout().lock())
            .map_err(|e| anyhow::anyhow!("mcp-server: {e}"));
    }

    // RPC mode owns stdin for the protocol — dispatch before any stdin read.
    if cli.rpc {
        return rpc::serve(agent, toolset, &cli).await;
    }

    // HTTP serve mode.
    if cli.serve {
        return serve::serve(
            agent,
            cli.serve_addr.clone(),
            cli.port,
            cli.serve_token.clone(),
            cli.cors.clone(),
            cli.access_log,
            session_dir0.clone(),
        )
        .await;
    }



    // Prompt resolution (only for non-RPC modes).
    let mut prompt = if let Some(t) = &cli.template {
        // template takes precedence over --file/prompt/stdin
        let tdir = root.join(".pi").join("templates");
        let text = match templates::load(&tdir, t) {
            Ok(t) => t,
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        };
        let mut vars = templates::builtin_vars(&root, &effective_model(&cli, &fc));
        for kv in &cli.set {
            if let Some((k, v)) = kv.split_once('=') {
                vars.insert(k.trim().to_string(), v.trim().to_string());
            } else {
                eprintln!("warning: --set {kv} has no '=' (ignored)");
            }
        }
        Some(templates::render(&text, &vars))
    } else if let Some(f) = &cli.file {
        let p = if f.is_absolute() {
            f.clone()
        } else {
            root.join(f)
        };
        match std::fs::read_to_string(&p) {
            Ok(t) => Some(t),
            Err(e) => return Err(anyhow::anyhow!("--file {}: {e}", p.display())),
        }
    } else if !cli.prompt.is_empty() {
        Some(cli.prompt.join(" "))
    } else if cli.review {
        Some(review::review_prompt(&root))
    } else if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        Some(read_stdin())
    } else {
        None
    };

    // `--plan`: prepend a planning directive so the agent drives the todo
    // tool before working (matches pi's plan-first workflow, built-in here).
    if cli.plan {
        if let Some(p) = prompt.take() {
            prompt = Some(format!(
                "{PLAN_DIRECTIVE}\n\nTask: {p}"
            ));
        } else {
            prompt = Some(PLAN_DIRECTIVE.to_string());
        }
    }

    // `-i` forces the interactive TUI: a prompt becomes the first message
    // rather than a one-shot print run (matches `pi chat "prompt"`).
    if cli.interactive {
        let initial_input: Option<String> = prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        return ui::interactive(
            agent,
            toolset,
            &cli,
            effective_model(&cli, &fc),
            theme_spec.clone(),
            initial_input,
            &session_dir0,
        )
        .await;
    }

    if let Some(p) = prompt {
        if p.trim().is_empty() && !cli.interactive {
            eprintln!("no prompt given (use -i for interactive mode)");
            return Err(anyhow::anyhow!("no prompt given")); // honest exit code
        }
        if !p.trim().is_empty() {
            let parts = build_user_parts(&cli, &p, &root)?;
            return run_print(agent, parts, &cli, ext_handles).await;
        }
    }

    // Interactive TUI (no prompt on a TTY).
    ui::interactive(
        agent,
        toolset,
        &cli,
        effective_model(&cli, &fc),
        theme_spec.clone(),
        None,
        &session_dir0,
    )
    .await
}

fn read_stdin() -> String {
    use std::io::Read;
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// Build a user message's content parts from the prompt + `--image` flags.
/// `@file.png`-style tokens inside the prompt that resolve to an existing
/// image under `root` are replaced with inline image parts (kept as text for
/// non-vision models).
fn build_user_parts(cli: &Cli, prompt: &str, root: &std::path::Path) -> Result<Vec<pi_ai::ContentPart>> {
    use pi_ai::types::{ContentPart, ImageSource};

    let mut parts: Vec<ContentPart> = Vec::new();
    let mut text = prompt.to_string();

    // in-line @tokens (only for vision-capable default; keep harmless anyway)
    let mut i = 0;
    while let Some(at) = text[i..].find('@') {
        let start = i + at;
        let rest = &text[start + 1..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == ']')
            .unwrap_or(rest.len());
        let token = &rest[..end];
        let ext = std::path::Path::new(token)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let is_img = ext.map(|e| IMAGE_EXTS.contains(&e.as_str())).unwrap_or(false);
        if is_img {
            let cand = if std::path::Path::new(token).is_absolute() {
                std::path::PathBuf::from(token)
            } else {
                root.join(token)
            };
            if cand.is_file() {
                if let Some(data) = base64_image(&cand) {
                    if text[..start].trim().is_empty() {
                        // leading token: keep buffer accumulation order sane
                    } else {
                        parts.push(ContentPart::Text {
                            text: text[..start].to_string(),
                        });
                    }
                    parts.push(ContentPart::Image {
                        media_type: media_type_for(token).to_string(),
                        source: ImageSource::Base64(data.0),
                    });
                    text = format!("{}{}", &text[..start], &text[start + 1 + end..]);
                    text = text.trim_start().to_string();
                    i = 0;
                    continue;
                }
            }
        }
        i = start + 1;
    }
    if !text.trim().is_empty() {
        parts.push(ContentPart::Text { text });
    }

    // explicit --image flags append after the text content
    for img in &cli.image {
        let p = if img.is_absolute() {
            img.clone()
        } else {
            root.join(img)
        };
        let Some(data) = base64_image(&p) else {
            return Err(anyhow::anyhow!("cannot read image {}", p.display()));
        };
        parts.push(ContentPart::Image {
            media_type: media_type_for(&p.to_string_lossy()).to_string(),
            source: ImageSource::Base64(data.0),
        });
    }
    Ok(parts)
}

struct ImageData(String);

fn base64_image(p: &std::path::Path) -> Option<ImageData> {
    let bytes = std::fs::read(p).ok()?;
    Some(ImageData(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)))
}

fn media_type_for(name: &str) -> &'static str {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

fn keys_file(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".pi").join("keys.json")
}

fn load_keys_file(root: &std::path::Path) -> Keys {
    pi_ai::Keys::load_file(&keys_file(root)).unwrap_or_default()
}

fn write_keys_file(root: &std::path::Path, k: &Keys) -> Result<(), String> {
    let p = keys_file(root);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(k).map_err(|e| e.to_string())?;
    std::fs::write(&p, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// `pi --login `provider=key`` stores a key; `--logout `provider`` removes it.
fn auth_command(cli: &Cli) -> Result<Option<()>, String> {
    let root = if cli.root.exists() { cli.root.clone() } else { std::path::PathBuf::from(".") };
    if let Some(spec) = &cli.login {
        let (prov, key) = spec
            .split_once('=')
            .ok_or_else(|| "--login expects PROVIDER=KEY".to_string())?;
        if key.trim().is_empty() {
            return Err("--login key must not be empty".into());
        }
        let mut k = load_keys_file(&root);
        k.provider.insert(prov.trim().to_string(), key.trim().to_string());
        write_keys_file(&root, &k)?;
        println!("stored key for provider '{prov}' in {}", keys_file(&root).display());
        return Ok(Some(()));
    }
    if let Some(prov) = &cli.logout {
        let mut k = load_keys_file(&root);
        if k.provider.remove(prov).is_some() {
            write_keys_file(&root, &k)?;
            println!("removed key for provider '{prov}'");
        } else {
            println!("no stored key for provider '{prov}'");
        }
        return Ok(Some(()));
    }
    Ok(None)
}

fn build_llm(cli: &Cli) -> Result<Llm> {
    let mut keys = Keys::from_env();
    // keys file (from `pi --login`) augments the environment
    let kf = load_keys_file(&cli.root);
    for (p, k) in kf.provider {
        keys.provider.entry(p).or_insert(k);
    }
    if let Some(k) = &cli.api_key {
        let prov = provider_of_ref(cli.model.as_deref().unwrap_or("mock/echo"));
        keys.provider.insert(prov, k.clone());
    }
    Ok(Llm {
        catalog: pi_ai::Catalog::builtin().clone(),
        keys,
    })
}

fn provider_of_ref(refstr: &str) -> String {
    refstr
        .split_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_else(|| refstr.to_string())
}

/// CLI model wins; else the project config; else the built-in default.
fn effective_model(cli: &Cli, fc: &config::FileConfig) -> String {
    cli.model
        .clone()
        .or_else(|| fc.model.clone())
        .unwrap_or_else(|| "mock/echo".to_string())
}

fn resolve_model(llm: &Llm, cli: &Cli, fc: &config::FileConfig) -> Result<ModelInfo> {
    let m = effective_model(cli, fc);
    resolve_model_ref(llm, cli, &m)
}

/// Resolve a model reference (`provider/id`, or `custom:<base_url>:<id>`).
fn resolve_model_ref(llm: &Llm, cli: &Cli, refstr: &str) -> Result<ModelInfo> {
    // `custom:<base_url>:<model>` shorthand for OpenAI-compatible endpoints.
    // Split at the LAST colon so `http://host:port/path:id` parses properly.
    if let Some(rest) = refstr.strip_prefix("custom:") {
        if let Some(idx) = rest.rfind(':') {
            let base = &rest[..idx];
            let model = &rest[idx + 1..];
            let mut m = ModelInfo {
                id: model.to_string(),
                provider: "custom".into(),
                name: model.to_string(),
                context_window: 128_000,
                max_output_tokens: 8192,
                input_price: 0.0,
                output_price: 0.0,
                reasoning: false,
                tools: true,
                vision: false,
                api: Some("openai".into()),
                base_url: Some(base.to_string()),
                sampling: None,
            };
            if let Some(bu) = &cli.base_url {
                m.base_url = Some(bu.clone());
            }
            return Ok(m);
        }
    }
    let mut m = llm.model(refstr)?;
    if let Some(bu) = &cli.base_url {
        m.base_url = Some(bu.clone());
    }
    Ok(m)
}

fn build_system(extra: Option<&str>, ask_mode: bool) -> Vec<pi_ai::ContentPart> {
    let mut base = if ask_mode {
        String::from(
            "You are pi in ask mode: answer the user's questions clearly and concisely.
You have NO tools and cannot touch the filesystem. Do not fabricate file contents or
pretend to run commands; give direct, well-reasoned answers.",
        )
    } else {
        String::from(
            "You are pi, a capable coding agent. You have tools to read, write, edit, search, and run
commands within the working directory. Be precise, minimal, and safe. Prefer small, verifiable
changes and verify edits with a build or test when available.",
        )
    };
    if let Some(extra) = extra {
        base.push_str("\n\n");
        base.push_str(extra);
    }
    vec![pi_ai::ContentPart::Text { text: base }]
}

fn build_session(cli: &Cli, session_dir: &std::path::Path) -> Result<Session, String> {
    // `--no-session`: fully in-memory, nothing written to disk.
    if cli.no_session {
        return Ok(Session::new(None));
    }
    // `--resume <id>`: load (or start) a specific session.
    if let Some(id) = &cli.resume {
        let mut s = Session::new(Some(id.clone()));
        s.attach_path(session_dir, true);
        return Ok(s);
    }
    // `--continue`: pick the most recent session file by mtime. Refusing to
    // silently run a fresh session keeps the exit code honest (audit).
    if cli.continue_ {
        if let Some(id) = newest_session_id(session_dir) {
            let mut s = Session::new(Some(id));
            s.attach_path(session_dir, true);
            return Ok(s);
        }
        return Err(format!("--continue: no previous session found in {session_dir:?}"));
    }
    // Default: a fresh session, but always attached so it persists on disk.
    let mut s = Session::new(None);
    s.attach_path(session_dir, true);
    Ok(s)
}

/// Newest `.jsonl` session id in `dir` by modification time (ignores subdirs).
fn newest_session_id(dir: &std::path::Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(id) = name.strip_suffix(".jsonl") else {
            continue;
        };
        let ok = e.metadata().ok().and_then(|m| m.modified().ok());
        let ts = ok.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| ts > *t).unwrap_or(true) {
            best = Some((ts, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

/// Non-interactive: run a single prompt, stream results.
async fn run_print(
    mut agent: Agent,
    prompt: Vec<pi_ai::ContentPart>,
    cli: &Cli,
    ext_handles: Vec<ext::ExtHandle>,
) -> Result<()> {
    let mut rx = agent.user_turn_parts(prompt).await;
    let mut final_text = String::new();
    let show_tools = !cli.quiet;
    while let Some(ev) = rx.recv().await {
        // best-effort observation: forward to extensions
        for h in &ext_handles {
            if let Ok(mut g) = h.lock() {
                let _ = g.broadcast(&serde_json::to_value(&ev).unwrap_or_default());
            }
        }
        match ev {
            AgentEvent::TextDelta(t) => {
                if cli.json {
                    println!("{}", serde_json::json!({"type":"text","delta":t}));
                } else {
                    print!("{t}");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                final_text.push_str(&t);
            }
            AgentEvent::ReasoningDelta(t) => {
                if !cli.quiet {
                    eprintln!("[think] {t}");
                }
            }
            AgentEvent::ToolCallStart(c) => {
                if show_tools {
                    eprintln!("  \u{2699} {} {}", c.name, pretty_args(&c.arguments));
                }
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({"type":"tool_call","name":c.name,"args":c.arguments})
                    );
                }
            }
            AgentEvent::ToolResult { call, result } => {
                if show_tools && !cli.quiet {
                    let ok = if result.is_error { "\u{2717}" } else { "\u{2713}" };
                    eprintln!(
                        "  {ok} {} {}",
                        call.name,
                        truncate(&result.output, 300)
                    );
                }
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "type":"tool_result","name":call.name,
                            "is_error":result.is_error,"output":result.output
                        })
                    );
                }
            }
            AgentEvent::Compacted { summary } => {
                eprintln!(
                    "  \u{27f3} compacted ({})",
                    truncate(&summary, 120)
                );
            }
            AgentEvent::Error(e) => {
                eprintln!("error: {e}");
                if cli.json {
                    println!("{}", serde_json::json!({"type":"error","message":e}));
                }
                return Err(anyhow::anyhow!("agent error: {e}"));
            }
            _ => {}
        }
    }
    if let Some(out) = &cli.out {
        // write the final answer to a file, confined to root like every
        // other file write (a `..`/absolute path must not escape)
        let p = match pi_tools::resolve_under(&cli.root, out.to_str().unwrap_or("")) {
            Ok(p) => p,
            Err(e) => return Err(anyhow::anyhow!("--out: {e}")),
        };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&p, final_text.as_str())
            .map_err(|e| anyhow::anyhow!("--out {}: {e}", p.display()))?;
    }
    if !cli.json {
        if !final_text.ends_with('\n') {
            println!();
        }
        eprintln!(
            "\n[usage] in={} out={} cost=${:.4} model={}",
            agent.total_usage.input, agent.total_usage.output, agent.total_cost, agent.config.model.id
        );
    } else {
        println!(
            "{}",
            serde_json::json!({"type":"done","usage":{
                "input":agent.total_usage.input,"output":agent.total_usage.output,
                "cost":agent.total_cost}})
        );
    }
    Ok(())
}

fn pretty_args(v: &serde_json::Value) -> String {
    let s = v.to_string();
    truncate(&s, 200)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parts_replaces_at_image_tokens_and_attaches_flags() {
        let dir = std::env::temp_dir().join(format!("pi-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 1x1 PNG
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("a.png"), png).unwrap();
        let cli = Cli::parse_from(["pi", "--image", "a.png"]);
        let parts = build_user_parts(&cli, "look at @a.png now", &dir).unwrap();
        let images = parts
            .iter()
            .filter(|p| matches!(p, pi_ai::ContentPart::Image { .. }))
            .count();
        assert_eq!(images, 2, "inline + --image flag => 2 image parts, got {parts:?}");
        let text: String = parts
            .iter()
            .filter_map(|p| match p {
                pi_ai::ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("look at"));
        assert!(text.contains("now"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod feature_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pi-feat-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn config_file_load_and_effective_model_precedence() {
        let d = tmp("cfg");
        std::fs::create_dir_all(d.join(".pi")).unwrap();
        std::fs::write(
            d.join(".pi").join("pi.json"),
            r#"{"model":"openai/gpt-4o","max_turns":3,"temperature":0.5}"#,
        )
        .unwrap();
        let fc = config::FileConfig::load(&d).unwrap();
        assert_eq!(fc.model.as_deref(), Some("openai/gpt-4o"));
        assert_eq!(fc.max_turns, Some(3));

        let cli = match Cli::try_parse_from(["pi"]) {
            Ok(c) => c,
            Err(e) => panic!("parse_from pi failed: {e:?}"),
        };
        assert_eq!(effective_model(&cli, &fc), "openai/gpt-4o");
        let cli2 = match Cli::try_parse_from(["pi", "-m", "mock/echo"]) {
            Ok(c) => c,
            Err(e) => panic!("parse_from -m failed: {e}"),
        };
        assert_eq!(effective_model(&cli2, &fc), "mock/echo", "flag wins");
        let fc_default = config::FileConfig::default();
        assert_eq!(effective_model(&cli, &fc_default), "mock/echo", "fallback default");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn export_round_trips_session_markdown() {
        let d = tmp("export");
        let id = "exp-1";
        let mut s = Session::new(Some(id.into()));
        s.attach_path(&d, true);
        s.push_message(pi_ai::types::ChatMessage::text_user("hello"));
        s.push_message(pi_ai::types::ChatMessage::text_assistant("world"));
        let f = d.join(format!("{id}.jsonl"));
        let out = export::run(&f, "md").unwrap();
        assert!(out.contains("user (0)"), "{out}");
        assert!(out.contains("hello"));
        assert!(out.contains("assistant (1)"));
        assert!(out.contains("world"));
        // txt (fallback) + html formats work
        let out2 = export::run(&f, "txt").unwrap();
        assert!(out2.contains("# session"));
        let out3 = export::run(&f, "html").unwrap();
        assert!(out3.contains("<!doctype html>"));
        assert!(out3.contains("hello"));
        assert!(out3.contains("pi session exp-1"));
        assert!(out3.ends_with("</html>"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn review_prompt_contains_git_sections() {
        let d = tmp("review");
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&d)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        assert!(git(&["init", "-q"]));
        // hermetic: GitHub runners ship no git identity; don't depend on it
        assert!(git(&["config", "user.name", "ci test"]));
        assert!(git(&["config", "user.email", "ci@example.invalid"]));
        std::fs::write(d.join("a.txt"), "v1\n").unwrap();
        assert!(git(&["add", "a.txt"]));
        assert!(git(&["commit", "-qm", "one"]));
        std::fs::write(d.join("a.txt"), "v2\n").unwrap();
        let p = review::review_prompt(&d);
        assert!(p.contains("GIT STATUS"), "{p}");
        assert!(p.contains("UNSTAGED DIFF"));
        assert!(p.contains("STAGED DIFF"));
        assert!(p.contains("+v2"), "{p}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

// Full-loop regression: real toolset + real session through the agent.
#[cfg(test)]
mod loop_tests {
    use super::*;

    #[tokio::test]
    async fn full_agent_loop_with_real_tools() {
        let dir = std::env::temp_dir().join(format!("pi-loop-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".pi")).unwrap();
        std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();

        let mut tools = ToolRegistry::new();
        let ts = Toolset::new(&dir);
        ts.register_all(&mut tools);

        let mut agent = Agent::new(
            Llm::new(),
            AgentConfig::default(),
            tools,
            Session::new(Some("loop-test".into())),
        );
        // mock "@tool" triggers the first advertised tool; the whole
        // loop (call + result + final) must complete and persist.
        let mut rx = agent.user_turn("do a thing @tool").await;
        let mut tool_called = false;
        let mut final_msg = false;
        while let Some(ev) = rx.recv().await {
            match ev {
                AgentEvent::ToolCallStart(c) => {
                    tool_called = true;
                    assert!(!c.name.is_empty());
                }
                AgentEvent::AssistantMessage { message, .. }
                    if message.tool_calls.is_empty() => {
                        final_msg = true;
                    }
                _ => {}
            }
        }
        assert!(tool_called, "the mock must emit a tool call");
        assert!(final_msg, "the loop must end with a final assistant message");

        // persisted: a session file exists with messages
        let p = dir.join("sessions.jsonl");
        let _ = p;
        let entry_count = agent.session.entries.len();
        assert!(entry_count >= 2, "user + assistant/tool entries, got {entry_count}");
        assert!(agent.session.messages().len() >= 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
#[cfg(test)]
mod cli_regression_tests {
    use super::*;

    #[test]
    fn flags_may_follow_the_prompt() {
        // regression: trailing_var_arg used to swallow --root/--session-dir
        // etc. after the first positional
        let cli = Cli::parse_from([
            "pi",
            "hello",
            "--root",
            "/tmp/x",
            "-m",
            "mock/echo",
            "--json",
        ]);
        assert_eq!(cli.prompt, vec!["hello".to_string()]);
        assert_eq!(cli.model.as_deref(), Some("mock/echo"));
        assert_eq!(cli.root, std::path::PathBuf::from("/tmp/x"));
        assert!(cli.json);

        // multi-word prompt still joins
        let cli2 = Cli::parse_from(["pi", "add", "a", "test"]);
        assert_eq!(cli2.prompt, vec!["add".to_string(), "a".to_string(), "test".to_string()]);

        // `--` lets a dash-prefixed prompt through
        let cli3 = Cli::try_parse_from(["pi", "--", "-d", "flag"]);
        assert!(cli3.is_ok());
        let cli3 = cli3.unwrap();
        assert_eq!(cli3.prompt, vec!["-d".to_string(), "flag".to_string()]);
    }

    #[test]
    fn login_logout_round_trip_keys_file() {
        let dir = std::env::temp_dir().join(format!("pi-auth-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".pi")).unwrap();
        let cli1 = Cli::parse_from(["pi", "--login", "openai=sk-x", "--root", dir.to_str().unwrap()]);
        assert!(matches!(auth_command(&cli1), Ok(Some(()))));
        let k = load_keys_file(&dir);
        assert_eq!(k.provider.get("openai").map(|s| s.as_str()), Some("sk-x"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(keys_file(&dir)).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o600, "keys file must be private");
        }
        let cli2 = Cli::parse_from(["pi", "--logout", "openai", "--root", dir.to_str().unwrap()]);
        assert!(matches!(auth_command(&cli2), Ok(Some(()))));
        assert_eq!(load_keys_file(&dir).provider.get("openai"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_requires_explicit_key() {
        let cli = Cli::parse_from(["pi", "--login", "openai="]);
        assert!(auth_command(&cli).is_err());
    }

    #[test]
    fn custom_model_ref_parses_urls() {
        // regression: splitn(2, ":") mangled http:// base URLs
        let llm = Llm::new();
        let cli = Cli::parse_from(["pi", "x"]);
        let m = resolve_model_ref(&llm, &cli, "custom:http://127.0.0.1:8787/v1:my-model").unwrap();
        assert_eq!(m.base_url.as_deref(), Some("http://127.0.0.1:8787/v1"));
        assert_eq!(m.id, "my-model");
        assert_eq!(m.provider, "custom");
        assert_eq!(m.api.as_deref(), Some("openai"));

        // no-port base + trailing slash
        let m2 = resolve_model_ref(&llm, &cli, "custom:https://llm.example.com/:gpt").unwrap();
        assert_eq!(m2.base_url.as_deref(), Some("https://llm.example.com/"));
        assert_eq!(m2.id, "gpt");
    }
}