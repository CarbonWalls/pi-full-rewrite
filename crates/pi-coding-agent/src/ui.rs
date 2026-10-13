//! Interactive TUI: transcript + spinner + editor, driven by [`Tui`] and the
//! owned [`Turn`] so the agent works in the background.

use anyhow::Result;
use crossterm::event::KeyCode;
use pi_agent::{Agent, AgentEvent, Turn};
use pi_tools::Toolset;
use pi_tui::editor::Editor;
use pi_tui::spinner::Spinner;
use pi_tui::tui::{Key, KeyEvent, Tui};
use pi_tui::widget::{Area, Widget};
use pi_tui::Buffer;
use std::collections::VecDeque;
use std::time::Duration;

const SHIFT: u8 = 0x01;
const CONTROL: u8 = 0x02;

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}\u{2026}")
    }
}

pub async fn interactive(
    agent: Agent,
    _toolset: Option<Toolset>,
    cli: &crate::Cli,
    initial_model: String,
    theme_spec: Option<String>,
    initial_input: Option<String>,
) -> Result<()> {
    // Shared submit path for editor Enter, pasted, and queued input.
    // Many refs are unavoidable — this is the single funnel for input.
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_input(
        text: String,
        turn: &mut Option<Turn>,
        agent: &mut Option<Agent>,
        queued: &mut VecDeque<String>,
        spinner: &mut Spinner,
        model_ref: &mut String,
        transcript: &mut String,
        session_dir: &std::path::Path,
        cli_root: &std::path::Path,
        theme: &mut pi_tui::Theme,
        fs: &mut FindState,
        tui_h: usize,
        scroll_back: &mut usize,
    ) -> bool {
        // returns true when the user asked to leave (e.g. /exit)
        if turn.is_some() {
            // busy: queue it (commands included, run when idle)
            queued.push_back(text);
            return false;
        }
        if let Some(cmd) = text.strip_prefix('/') {
            if let Some(a) = agent.as_mut() {
                match run_command(
                    cmd,
                    a,
                    model_ref,
                    transcript,
                    false,
                    session_dir,
                    cli_root,
                )
                .await
                {
                    CmdOutcome::Exit => return true,
                    CmdOutcome::Prompt(prompt) => {
                        if let Some(a2) = agent.take() {
                            spinner.set_label("reviewing…");
                            spinner.spin = true;
                            *turn = Some(a2.start_turn(prompt));
                        } else {
                            transcript.push_str("\n(busy, try again)\n");
                        }
                    }
                    CmdOutcome::Theme(spec) => {
                        *theme = pi_tui::Theme::resolve(Some(&spec));
                        transcript.push_str(&format!("\n🎨 theme → {spec}\n"));
                    }
                    CmdOutcome::Find(idx, q) => {
                        match q {
                            Some(query) if !idx.is_empty() => {
                                transcript.push_str(&format!(
                                    "\n(find: {} × “{query}”; /find = next)\n",
                                    idx.len()
                                ));
                                let total_hint = transcript.lines().count().max(1);
                                fs.pos = 0;
                                fs.indices = idx.clone();
                                fs.query = query.clone();
                                let target = idx[0];
                                *scroll_back =
                                    total_hint.saturating_sub(tui_h).saturating_sub(target);
                            }
                            Some(query) => {
                                transcript.push_str(&format!("\n(find: no matches for “{query}”)\n"));
                                fs.indices.clear();
                                fs.query.clear();
                            }
                            None => {
                                // step to the next stored match
                                if fs.indices.is_empty() {
                                    transcript.push_str("\n(find: no active search; use /find <text>)\n");
                                } else {
                                    fs.pos = (fs.pos + 1) % fs.indices.len();
                                    let total_hint = transcript.lines().count().max(1);
                                    let target = fs.indices[fs.pos];
                                    *scroll_back =
                                        total_hint.saturating_sub(tui_h).saturating_sub(target);
                                    transcript.push_str(&format!(
                                        "\n(find {}/{} — “{}”)\n",
                                        fs.pos + 1,
                                        fs.indices.len(),
                                        fs.query
                                    ));
                                }
                            }
                        }
                    }
                    CmdOutcome::None => {}
                }
            } else {
                transcript.push_str("\n(busy)\n");
            }
        } else if let Some(a) = agent.take() {
            spinner.set_label("working…");
            spinner.spin = true;
            *turn = Some(a.start_turn(text));
        } else {
            transcript.push_str("\n(busy, try again)\n");
        }
        false
    }

    /// Submit from the editor/paste: preserve ordering when the queue already
    /// has pending items (e.g. an initial prompt from `pi -i "msg"`), so a
    /// fast `/exit` can't jump the line.
    #[allow(clippy::too_many_arguments)]
    async fn submit_input(
        text: String,
        turn: &mut Option<Turn>,
        agent: &mut Option<Agent>,
        queued: &mut VecDeque<String>,
        spinner: &mut Spinner,
        model_ref: &mut String,
        transcript: &mut String,
        session_dir: &std::path::Path,
        cli_root: &std::path::Path,
        theme: &mut pi_tui::Theme,
        fs: &mut FindState,
        tui_h: usize,
        scroll_back: &mut usize,
    ) -> bool {
        if turn.is_some() || !queued.is_empty() {
            queued.push_back(text);
            return false;
        }
        dispatch_input(
            text,
            turn,
            agent,
            queued,
            spinner,
            model_ref,
            transcript,
            session_dir,
            cli_root,
            theme,
            fs,
            tui_h,
            scroll_back,
        )
        .await
    }

    let mut tui = Tui::start()?;
    // Ownership of the agent flips between the loop and the running turn:
    // `None` while a turn is in flight, `Some(agent)` when we hold it.

    let mut editor = Editor::new();
    editor.placeholder = "message pi…  · /help  · /model <ref>  · Ctrl-C quits".into();
    let mut spinner = Spinner::new("ready");
    let mut transcript = String::new();
    transcript.push_str(&format!(
        "# pi (rust)\nmodel: **{}/{}**  ·  `root {}`  ·  {} tools\n\n*type /help for
commands*\n",
        agent.config.model.provider,
        agent.config.model.id,
        cli.root.display(),
        agent.tools.names().len()
    ));

    let mut model_ref = initial_model;
    let session_dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| cli.root.join(".pi").join("sessions"));
    let cli_root = cli.root.clone();
    let mut theme = pi_tui::Theme::resolve(theme_spec.as_deref());
    let mut scroll_back = 0usize;
    let mut fs = FindState { indices: Vec::new(), pos: 0, query: String::new() };
    let mut agent = Some(agent);
    let session_id = agent.as_ref().map(|a| a.session.id.clone()).unwrap_or_default();
    let abort_handle = agent.as_ref().unwrap().abort.clone();
    // SIGTERM -> graceful exit (restores the terminal). A watch channel lets
    // the sync event loop see the flag without async signal polling.
    let (term_tx, term_rx) = tokio::sync::watch::channel(false);
    #[cfg(unix)]
    if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        let tx = term_tx.clone();
        tokio::spawn(async move {
            while sig.recv().await.is_some() {
                let _ = tx.send(true);
            }
        });
    }
    let mut turn: Option<Turn> = None;
    let mut queued: VecDeque<String> = VecDeque::new();
    // `pi -i "prompt"`: the prompt becomes the first submitted message.
    if let Some(p) = initial_input.as_deref() {
        queued.push_back(p.to_string());
        transcript.push_str("\n*initial prompt queued*\n");
    }

    'main: loop {
        // SIGTERM (e.g. a supervisor killing the pty): exit gracefully so
        // the terminal is restored (leave alternate screen + raw mode off).
        if *term_rx.borrow() {
            break 'main;
        }

        // Reclaim the agent once its turn is fully finished.
        if let Some(mut t) = turn.take() {
            if t.closed() {
                // Drain the buffered tail of events before dropping the
                // receiver — otherwise an instantly-completed turn (e.g.
                // the mock provider) loses its whole transcript.
                while let Some(ev) = t.try_recv() {
                    apply_event(&mut transcript, ev);
                }
                let mut reclaimed = t.finish().await;
                if let Ok(m) = reclaimed.llm.model(&model_ref) {
                    reclaimed.config.model = m;
                }
                reclaimed.clear_abort();
                agent = Some(reclaimed);
                spinner.set_label("ready");
                spinner.spin = false;
            } else {
                turn = Some(t);
            }
        }

        // Drain the queued input whenever idle (commands like /exit must run
        // even when they were queued while a turn was in flight; a queued
        // message starts the next turn).
        if turn.is_none() {
            if let Some(q) = queued.pop_front() {
                scroll_back = 0; // new input: follow the tail again
                if dispatch_input(
                    q,
                    &mut turn,
                    &mut agent,
                    &mut queued,
                    &mut spinner,
                    &mut model_ref,
                    &mut transcript,
                    &session_dir,
                    &cli_root,
                    &mut theme,
                    &mut fs,
                    tui.size().1 as usize,
                    &mut scroll_back,
                )
                .await
                {
                    break 'main;
                }
                continue; // re-evaluate (a message just started a turn)
            }
        }

        // Drain pending agent events into the transcript.
        if let Some(t) = turn.as_mut() {
            while let Some(ev) = t.try_recv() {
                apply_event(&mut transcript, ev);
            }
        }

        // Consume one TUI event (short poll keeps the loop responsive).
        if let Ok(Some(ev)) = tui.poll_event(Some(Duration::from_millis(15))) {
            if let Ok(path) = std::env::var("PI_DEBUG_EVENTS") {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = writeln!(f, "{:?}", ev.key);
                }
            }
            match &ev.key {
                // crossterm reports real Ctrl-C (0x03) as Char('c')+CONTROL.
                Key::Code(KeyCode::Char('c')) if ev.mods & CONTROL == CONTROL => {
                    // Ctrl-C: abort an in-flight turn, else quit.
                    if turn.is_some() {
                        abort_handle.store(true, std::sync::atomic::Ordering::Relaxed);
                        transcript.push_str("\n⟳ aborting… (Ctrl-C again to quit)\n");
                    } else {
                        break 'main;
                    }
                }
                _ => {}
            }
            if let Key::Code(c) = &ev.key {
                let c = *c;
                // Tab: complete a /command prefix against the built-in list
                if c == KeyCode::Tab {
                    complete_slash(&mut editor);
                    continue;
                }
                // PgUp/PgDn: scroll back through the transcript history
                let trans_h_page = (tui.size().1 as usize).saturating_sub(3).max(1);
                if c == KeyCode::PageUp {
                    scroll_back = scroll_back.saturating_add(trans_h_page);
                    continue;
                }
                if c == KeyCode::PageDown {
                    scroll_back = scroll_back.saturating_sub(trans_h_page);
                    continue;
                }
                let was_single_line = editor.lines.len() == 1;
                if c == KeyCode::Enter && ev.mods & SHIFT != 0 {
                    editor.newline();
                } else {
                    let _ = editor.handle_key(c);
                }
                // Plain Enter on a single line = submit.
                if c == KeyCode::Enter && was_single_line && editor.lines.len() == 1 {
                    if let Some(text) = editor.commit() {
                        scroll_back = 0;
                        if submit_input(
                            text,
                            &mut turn,
                            &mut agent,
                            &mut queued,
                            &mut spinner,
                            &mut model_ref,
                            &mut transcript,
                            &session_dir,
                            &cli_root,
                            &mut theme,
                            &mut fs,
                            tui.size().1 as usize,
                            &mut scroll_back,
                        )
                        .await
                        {
                            break 'main;
                        }
                    }
                }
            } else if let Key::Paste(s) = &ev.key {
                // Piped/scripted input arrives as one paste. Split it into
                // lines and submit each (this is how `x | pi -i` flows through
                // a PTY). A paste without newlines inserts into the editor.
                let mut had_nl = false;
                let mut lines: Vec<String> = Vec::new();
                let mut cur = String::new();
                for ch in s.chars() {
                    if ch == '\n' || ch == '\r' {
                        had_nl = true;
                        lines.push(std::mem::take(&mut cur));
                    } else {
                        cur.push(ch);
                    }
                }
                if !cur.is_empty() {
                    lines.push(cur);
                }
                if had_nl {
                    for line in lines {
                        if line.trim().is_empty() {
                            continue;
                        }
                        editor.set_line(line.clone());
                        if let Some(text) = editor.commit() {
                            scroll_back = 0;
                            if submit_input(
                                text,
                                &mut turn,
                                &mut agent,
                                &mut queued,
                                &mut spinner,
                                &mut model_ref,
                                &mut transcript,
                                &session_dir,
                                &cli_root,
                                &mut theme,
                                &mut fs,
                                tui.size().1 as usize,
                                &mut scroll_back,
                            )
                            .await
                            {
                                break 'main;
                            }
                        }
                    }
                } else if !s.is_empty() {
                    let ev2 = KeyEvent {
                        key: Key::Paste(s.clone()),
                        mods: 0,
                    };
                    editor.on_event(&ev2);
                }
            }
        }

        // Render the frame.
        {
            let status_sid = agent
                .as_ref()
                .map(|a| a.session.id.clone())
                .unwrap_or_else(|| session_id.clone());
            let status = format!(
                "{model_ref}  ·  session {status_sid}{}",
                if scroll_back > 0 { "  ·  ^scroll" } else { "" }
            );
            let mut frame = Frame {
                transcript: &transcript,
                spinner: &mut spinner,
                editor: &mut editor,
                theme: &theme,
                scroll_back,
                status,
            };
            tui.render(&mut frame)?;
        }

        if turn.is_some() {
            spinner.tick();
        }
    }
    // leave the terminal in a sane state (raw mode + alternate screen off)
    let _ = tui.stop();
    Ok(())
}

fn apply_event(transcript: &mut String, ev: AgentEvent) {
    match ev {
        AgentEvent::UserMessage(m) => {
            transcript.push_str(&format!("\n❯ {}\n", m.text()));
        }
        AgentEvent::AssistantStart { model } => {
            transcript.push_str(&format!("\n✻ {model}…\n"));
        }
        AgentEvent::TextDelta(t) => transcript.push_str(&t),
        AgentEvent::ReasoningDelta(t) => transcript.push_str(&t),
        AgentEvent::ToolCallStart(c) => {
            let args = c.arguments.to_string();
            let shown: String = args.chars().take(120).collect();
            transcript.push_str(&format!("\n⏺ `{}` {}\n", c.name, shown));
        }
        AgentEvent::ToolResult { call, result } => {
            let mark = if result.is_error { "✗" } else { "✓" };
            let first: String = result
                .output
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(120)
                .collect();
            transcript.push_str(&format!("   {mark} {} · {first}\n", call.name));
        }
        AgentEvent::Compacted { summary } => {
            let s: String = summary.chars().take(200).collect();
            transcript.push_str(&format!("\n⟳ compacted: {s}\n"));
        }
        AgentEvent::Error(e) => transcript.push_str(&format!("\n⚠ {e}\n")),
        AgentEvent::AssistantMessage { .. } | AgentEvent::Usage(_) => {}
    }
}

/// In-TUI transcript search state (/find).
struct FindState {
    indices: Vec<usize>,
    pos: usize,
    query: String,
}

enum CmdOutcome {
    None,
    /// Run a turn with this prompt (e.g. /review).
    Prompt(String),
    /// Leave the interactive loop cleanly (terminal restored via Tui::stop).
    Exit,
    /// Apply a theme spec at runtime (e.g. /theme one-dark).
    Theme(String),
    /// Search the transcript: (matching raw-line indices, query; None = step).
    Find(Vec<usize>, Option<String>),
}

async fn run_command(
    cmd: &str,
    agent: &mut Agent,
    model_ref: &mut String,
    transcript: &mut String,
    busy: bool,
    session_dir: &std::path::Path,
    root: &std::path::Path,
) -> CmdOutcome {
    let mut parts = cmd.splitn(2, ' ');
    let head = parts.next().unwrap_or("").trim();
    let arg = parts.next().unwrap_or("").trim();
    match head {
        "exit" | "quit" | "q" => { return CmdOutcome::Exit; }
        "help" | "?" => transcript.push_str(
            "\n**commands**: /help · /model <ref> · /compact · /cost · /tools · /session ·
/sessions · /git [args] · /label <name> · /theme <spec> · /find <text> · /new · /fork <n> · /replay <n> · /clear · /exit\n",
        ),
        "clear" | "reset" => {
            transcript.clear();
            transcript.push_str(&format!(
                "# pi (rust) — {}/{}\n",
                agent.config.model.provider, agent.config.model.id
            ));
        }
        "model" => {
            if arg.is_empty() {
                transcript.push_str(&format!("\nmodel: {model_ref}\n"));
            } else {
                // validate immediately so a bad ref gets a clear message
                match agent.llm.model(arg) {
                    Ok(m) => {
                        *model_ref = arg.to_string();
                        transcript.push_str(&format!(
                            "\nmodel → **{}/{}**\n",
                            m.provider, m.id
                        ));
                    }
                    Err(e) => {
                        transcript.push_str(&format!("\nmodel error: {e}\n"));
                    }
                }
            }
        }
        "review" => {
            let prompt = crate::review::review_prompt(root);
            transcript.push_str("\n⟳ reviewing working tree (git diff)\n");
            return CmdOutcome::Prompt(prompt);
        }
        "new" => {
            if busy {
                transcript.push_str("\n(new session) wait for the current turn to finish\n");
            } else {
                agent.session = pi_agent::session::Session::new(None);
                agent.session.attach_path(session_dir, true);
                agent.total_usage = Default::default();
                agent.total_cost = 0.0;
                transcript.clear();
                transcript.push_str(&format!(
                    "# pi (rust) — new session {}\n",
                    agent.session.id
                ));
            }
        }
        "fork" => {
            let at: Option<usize> = arg.trim().parse().ok();
            match at {
                Some(i) if i < agent.session.entries.len() => {
                    agent.session.fork_from(i);
                    transcript.push_str(&format!(
                        "\nforked: next message chains from 0-based entry {i} ({} entries kept tool-free before it)\n",
                        i
                    ));
                }
                _ => transcript.push_str(&format!(
                    "\nfork: bad index '{arg}' (have {} entries)\n",
                    agent.session.entries.len()
                )),
            }
        }
        "replay" => {
            let i: Option<usize> = arg.trim().parse().ok();
            match i {
                Some(idx) => match agent.session.entries.get(idx) {
                    Some(e) => {
                        let body = match &e.body {
                            pi_agent::session::Body::Message { msg } => msg.text(),
                            pi_agent::session::Body::Compaction { summary, .. } => {
                                summary.clone()
                            }
                            pi_agent::session::Body::Label { label } => label.clone(),
                            pi_agent::session::Body::Usage { input, output, cost, model } => {
                                format!("{model} in={input} out={output} cost=${cost:.6}")
                            }
                        };
                        transcript.push_str(&format!(
                            "\n[entry {idx}] {:?} {}\n",
                            e.kind,
                            truncate(&body, 400)
                        ));
                    }
                    None => transcript.push_str(&format!("\nreplay: no entry {idx}\n")),
                },
                None => transcript.push_str(
                    "\nreplay: usage /replay <entry-index> · /session shows the list\n",
                ),
            }
        }
        "compact" => {
            if busy {
                transcript.push_str("\n⟳ wait for the current turn to finish\n");
            } else {
                agent.compact_now().await;
                transcript.push_str("\n⟳ compacted\n");
            }
        }
        "cost" => {
            transcript.push_str(&format!("\n{}\n", agent.cost_dashboard()));
        }
        "tools" => {
            transcript.push_str(&format!("\ntools: {}\n", agent.tools.names().join(", ")));
        }
        "session" => {
            let p = agent.session.path.as_ref().map(|p| p.display().to_string());
            let label = agent.session.label().unwrap_or("-");
            let msgs = agent
                .session
                .messages()
                .iter()
                .filter(|m| matches!(m.role, pi_ai::types::Role::User))
                .count();
            transcript.push_str(&format!(
                "\nsession: {} · label: {} · {} entries · {msgs} user turns · file: {}\n",
                agent.session.id,
                label,
                agent.session.entries.len(),
                p.as_deref().unwrap_or("(in-memory)")
            ));
        }
        "sessions" => {
            let rows = crate::stats::list(session_dir);
            transcript.push_str(&format!("\n{}\n", crate::stats::render_list(&rows)));
        }
        "label" => {
            if arg.is_empty() {
                transcript.push_str("\nusage: /label <name>\n");
            } else {
                agent.session.push_label(arg.to_string());
                transcript.push_str(&format!("\n📛 session labeled: {arg}\n"));
            }
        }
        "theme" => {
            if arg.is_empty() {
                transcript.push_str("\nusage: /theme <inline-json | file.json>\n");
            } else {
                return CmdOutcome::Theme(arg.to_string());
            }
        }
        "find" => {
            if arg.is_empty() {
                return CmdOutcome::Find(Vec::new(), None); // step
            }
            let q = arg.trim().to_string();
            let idx: Vec<usize> = transcript
                .lines()
                .enumerate()
                .filter(|(_, l)| l.contains(&q))
                .map(|(i, _)| i)
                .collect();
            return CmdOutcome::Find(idx, Some(q));
        }
        "git" => {
            // quick read-only git access from the TUI (same whitelist as the
            // agent's git tool)
            let g = pi_tools::git::Git {
                root: root.to_path_buf(),
            };
            let spec = if arg.is_empty() { "status --short".into() } else { arg.to_string() };
            let res = pi_agent::tools::Tool::run(&g, serde_json::json!({"args": spec}));
            transcript.push_str(&format!("\n```\n$ git {spec}\n{}\n```\n", res.output));
        }
        unknown => transcript.push_str(&format!("\nunknown command: /{unknown} — /help\n")),
    };
    CmdOutcome::None
}

/// Tab-complete a `/command` prefix (or a model ref after `/model `) in the
/// editor.
fn complete_slash(editor: &mut pi_tui::editor::Editor) {
    const SLASH: [&str; 15] = [
        "help", "model", "compact", "cost", "tools", "session", "sessions",
        "git", "label", "new", "fork", "replay", "clear", "exit", "review",
    ];
    let line = editor.text();
    // model ref completion after `/model <prefix>`
    if let Some(prefix) = line.strip_prefix("/model ") {
        let models: Vec<String> = pi_ai::Catalog::builtin()
            .all()
            .iter()
            .map(|m| format!("{}/{}", m.provider, m.id))
            .filter(|refr| refr.starts_with(prefix))
            .collect();
        let matches: Vec<&str> = models.iter().map(|s| s.as_str()).collect();
        if !matches.is_empty() {
            if matches.len() == 1 {
                editor.set_line(format!("/model {} ", matches[0]));
            } else {
                let common = common_prefix(&matches);
                if common.len() > prefix.len() {
                    editor.set_line(format!("/model {common}"));
                }
            }
        }
        return;
    }
    if !line.starts_with('/') {
        return;
    }
    let rest = &line[1..];
    let matches: Vec<&str> = SLASH
        .iter()
        .copied()
        .filter(|c| c.starts_with(rest))
        .collect();
    match matches.len() {
        1 => editor.set_line(format!("/{} ", matches[0])),
        0 => {}
        _ => {
            // multiple: expand to the longest common prefix
            let prefix = common_prefix(&matches);
            if prefix.len() > rest.len() {
                editor.set_line(format!("/{prefix}"));
            }
        }
    }
}

/// Longest common prefix of a set of &str candidates.
fn common_prefix(candidates: &[&str]) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let first = candidates[0].as_bytes();
    let mut n = first.len();
    for c in &candidates[1..] {
        let cb = c.as_bytes();
        n = n.min(
            first
                .iter()
                .zip(cb.iter())
                .take_while(|(a, b)| a == b)
                .count(),
        );
        if n == 0 {
            break;
        }
    }
    String::from_utf8_lossy(&first[..n]).into_owned()
}

/// Composite top-level widget: transcript (markdown) + spinner + editor.
struct Frame<'a> {
    transcript: &'a str,
    spinner: &'a mut Spinner,
    editor: &'a mut Editor,
    theme: &'a pi_tui::Theme,
    scroll_back: usize,
    status: String,
}

impl<'a> Widget for Frame<'a> {
    fn height(&self, _w: usize) -> usize {
        0
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area) {
        let h = area.h;
        let w = area.w;
        let edit_h = self.editor.lines.len().max(1).min(h.saturating_sub(2));
        let spin_row = h.saturating_sub(edit_h).saturating_sub(1);
        let trans_h = spin_row;
        if trans_h > 0 {
            let ta = Area::new(0, 0, w, trans_h);
            pi_tui::draw_markdown_scroll_with(
                buf,
                &ta,
                self.transcript,
                trans_h,
                self.theme,
                self.scroll_back,
            );
        }
        if spin_row < h {
            let sa = Area::new(0, spin_row, w, 1);
            self.spinner.render(buf, &sa);
            // status suffix: model · session · scroll indicator
            let x = self.spinner.width().min(sa.w);
            if x + 2 < sa.w {
                pin_tui_status(
                    buf,
                    sa.x + x + 1,
                    spin_row,
                    &self.status,
                    sa.x + sa.w,
                    self.theme,
                );
            }
        }
        if spin_row + 1 < h {
            let ea = Area::new(0, spin_row + 1, w, edit_h);
            self.editor.render(buf, &ea);
        }
    }
}

/// Render the status suffix (model · session · scroll) after the spinner.
fn pin_tui_status(
    buf: &mut pi_tui::Buffer,
    x: usize,
    y: usize,
    status: &str,
    max_x: usize,
    theme: &pi_tui::Theme,
) {
    let mut c = theme.dim;
    c.bold = false;
    let avail = max_x.saturating_sub(x).max(1);
    let text: String = status.chars().take(avail).collect();
    buf.print(x, y, &text, &c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_tui::editor::Editor;

    #[test]
    fn tab_completes_full_command() {
        let mut e = Editor::new();
        for ch in "/comp".chars() {
            e.handle_key(crossterm::event::KeyCode::Char(ch));
        }
        complete_slash(&mut e);
        assert!(e.text().starts_with("/compact"), "completed: {}", e.text());
    }

    #[test]
    fn tab_expands_common_prefix() {
        let mut e = Editor::new();
        for ch in "/s".chars() {
            e.handle_key(crossterm::event::KeyCode::Char(ch));
        }
        complete_slash(&mut e);
        // session + sessions share the "/session" prefix
        assert!(e.text().starts_with("/session"), "expanded: {}", e.text());
    }

    #[test]
    fn tab_ignores_non_slash() {
        let mut e = Editor::new();
        for ch in "hello".chars() {
            e.handle_key(crossterm::event::KeyCode::Char(ch));
        }
        complete_slash(&mut e);
        assert_eq!(e.text(), "hello");
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;
    use pi_tui::editor::Editor;

    #[test]
    fn tab_completes_model_ref() {
        let mut e = Editor::new();
        for ch in "/model moc".chars() {
            e.handle_key(crossterm::event::KeyCode::Char(ch));
        }
        complete_slash(&mut e);
        assert_eq!(e.text(), "/model mock/echo ", "mock completes uniquely");
    }

    #[test]
    fn common_prefix_shared() {
        assert_eq!(common_prefix(&["session", "sessions"]), "session");
        assert_eq!(common_prefix(&["openai/gpt-4o", "openai/gpt-4o-mini"]), "openai/gpt-4o");
        assert_eq!(common_prefix(&["a", "b"]), "");
    }
}
