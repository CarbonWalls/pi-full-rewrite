//! `pi doctor`: environment diagnostics — tool availability, git repo,
//! network/proxy hints, disk space, config health.

use std::process::Command;

pub struct Report {
    pub checks: Vec<(String, bool, String)>,
}

impl Report {
    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn run(root: &std::path::Path, session_dir: Option<&std::path::Path>) -> Report {
        let mut checks = Vec::new();

        // required binaries
        for (name, args) in [
            ("git", vec!["--version"]),
            ("sh", vec!["-c", "true"]),
            ("python3", vec!["--version"]),
            ("node", vec!["--version"]),
        ] {
            let ok = Command::new(name).args(&args).output().map(|o| o.status.success()).unwrap_or(false);
            let detail = if ok { "ok".into() } else { format!("{name} not found on PATH") };
            checks.push((name.to_string(), ok, detail));
        }

        // working directory
        let cwd_ok = root.is_dir();
        checks.push(("working dir".into(), cwd_ok, root.display().to_string()));

        // git repo
        let git_ok = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(root)
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("true"))
            .unwrap_or(false);
        checks.push((
            "git repo".into(),
            git_ok,
            if git_ok { "clean working tree check: git status" } else { "not a git repo (fine for plain dirs)" }.into(),
        ));

        // disk space
        let disk = free_bytes(root).unwrap_or(0);
        let disk_ok = disk > 100 * 1024 * 1024;
        checks.push((
            "disk space".into(),
            disk_ok,
            format!("{} MB free", disk / 1024 / 1024),
        ));

        // stdio tty (interactive mode)
        let tty = std::io::IsTerminal::is_terminal(&std::io::stdin())
            && std::io::IsTerminal::is_terminal(&std::io::stdout());
        checks.push((
            "terminal".into(),
            tty,
            if tty { "interactive TUI available" } else { "pipelines/non-tty: use print or --rpc modes" }.into(),
        ));

        // api keys present in env or stored via --login in <root>/.pi/keys.json
        let mut keys = 0;
        for name in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "OPENROUTER_API_KEY"] {
            if std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false) {
                keys += 1;
            }
        }
        let keys_src: &'static str = if keys > 0 {
            "env"
        } else {
            let j = std::fs::read_to_string(crate::keys_file(&root)).and_then(|t| {
                serde_json::from_str::<serde_json::Value>(&t).map_err(|e| std::io::Error::new(
                    std::io::ErrorKind::InvalidData, e))
            }).unwrap_or(serde_json::Value::Null);
            let n = j.get("provider").and_then(|p| p.as_object()).map(|m| m.len()).unwrap_or(0);
            keys = n;
            if n > 0 { "keys.json" } else { "none" }
        };
        checks.push((
            "api keys".into(),
            keys > 0,
            if keys > 0 {
                format!("{keys} provider key(s) present ({keys_src})")
            } else {
                "no provider keys (env or .pi/keys.json; mock provider still works)".to_string()
            },
        ));

        // config file parses
        let cfg_path = root.join(".pi").join("pi.json");
        match std::fs::read_to_string(&cfg_path) {
            Ok(_) => match crate::config::FileConfig::load(root) {
                Ok(_) => checks.push(("pi.json".into(), true, "parses cleanly".into())),
                Err(e) => checks.push(("pi.json".into(), false, format!("parse error: {e}"))),
            },
            Err(_) => checks.push(("pi.json".into(), true, "optional (absent)".into())),
        }

        // session files not pathologically large
        let sd = session_dir
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| root.join(".pi").join("sessions"));
        let mut largest = 0u64;
        if let Ok(rd) = std::fs::read_dir(&sd) {
            for e in rd.flatten() {
                if let Some(m) = e.metadata().ok().map(|m| m.len()) {
                    largest = largest.max(m);
                }
            }
        }
        checks.push((
            "session size".into(),
            largest < 10 * 1024 * 1024,
            if largest > 0 {
                format!("largest session: {} KB", largest / 1024)
            } else {
                "no sessions yet".into()
            },
        ));

        // cumulative cost from recorded usage entries
        let (_, (_, _, cost), calls) = crate::stats::aggregate_cost(&sd);
        checks.push((
            "cost".into(),
            true,
            if calls > 0 {
                format!("{calls} model calls · ${cost:.6}")
            } else {
                "no usage entries yet".into()
            },
        ));

        // session dir writable (the same dir the session checks use)
        let sd_ok = std::fs::create_dir_all(&sd).is_ok()
            && std::fs::File::create(sd.join(".doctor-probe")).is_ok();
        let _ = std::fs::remove_file(sd.join(".doctor-probe"));
        checks.push((
            "session dir".into(),
            sd_ok,
            if sd_ok { sd.display().to_string() } else { "not writable".to_string() },
        ));

        // git identity present (a committed-less git env breaks --review/commit)
        let gname = std::process::Command::new("git")
            .args(["config", "user.name"]).output().map(|o| o.status.success()).unwrap_or(false);
        let gmail = std::process::Command::new("git")
            .args(["config", "user.email"]).output().map(|o| o.status.success()).unwrap_or(false);
        checks.push((
            "git identity".into(),
            gname && gmail,
            if gname && gmail { "user.name + user.email set".into() }
            else { "set git config user.name/email (needed for --review)".into() },
        ));

        // an offline mock turn completes (the whole local stack works)
        let mock_ok = {
            let dir = std::env::temp_dir().join(format!("pi-dr-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let r = std::process::Command::new(std::env::current_exe().unwrap_or_default())
                .args(["--quiet", "--root", dir.to_str().unwrap_or(""), "-m", "mock/echo", "doctor-probe"])
                .stdin(std::process::Stdio::null()).output();
            let _ = std::fs::remove_dir_all(&dir);
            r.map(|o| o.status.success()).unwrap_or(false)
        };
        checks.push((
            "mock turn".into(),
            mock_ok,
            if mock_ok { "offline echo turn completed".into() } else { "offline stack broken".into() },
        ));

        Report { checks }
    }

    pub fn render(&self) -> String {
        let mut out = format!("pi {} doctor\n", Self::version());
        out.push_str("check            status  detail\n----------------|--------|------------------------------\n");
        for (name, ok, detail) in &self.checks {
            out.push_str(&format!("{name:<16} {}  {detail}\n", if *ok { "ok " } else { "FAIL" }));
        }
        out
    }
}

fn free_bytes(dir: &std::path::Path) -> Option<u64> {
    // `df -k <dir>` yields: Filesystem 1024-blocks Used Available Capacity Mounted on
    let out = Command::new("df").args(["-k", dir.to_str()?]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    // Available is the 4th field
    let avail: u64 = fields.get(3)?.parse().ok()?;
    Some(avail * 1024)
}
