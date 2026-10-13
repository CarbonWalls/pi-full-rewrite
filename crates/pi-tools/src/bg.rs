//! Background bash: start a job, poll/kill it, read its output.
//!
//! Jobs run on detached std threads; output is drained into shared, capped
//! buffers plus spill files so the tool registry stays the single owner.

use pi_agent::tools::{Tool, ToolResult};
use pi_ai::types::ToolSpec;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const CAP: usize = 20_000;

struct JobBuf {
    out: Mutex<String>,
    err: Mutex<String>,
    spill_out: Option<Mutex<std::fs::File>>,
    spill_err: Option<Mutex<std::fs::File>>,
}

impl JobBuf {
    fn push_out(&self, s: &str) {
        let mut g = self.out.lock().unwrap();
        let room = CAP.saturating_sub(g.len());
        if room > 0 {
            g.push_str(&s.chars().take(room).collect::<String>());
        }
        if let Some(f) = &self.spill_out {
            let mut f = f.lock().unwrap();
            let _ = f.write_all(s.as_bytes());
        }
    }
    fn push_err(&self, s: &str) {
        let mut g = self.err.lock().unwrap();
        let room = CAP.saturating_sub(g.len());
        if room > 0 {
            g.push_str(&s.chars().take(room).collect::<String>());
        }
        if let Some(f) = &self.spill_err {
            let mut f = f.lock().unwrap();
            let _ = f.write_all(s.as_bytes());
        }
    }
}

pub struct BgJob {
    pub id: String,
    pub command: String,
    child: Option<Child>,
    buf: Arc<JobBuf>,
    done: bool,
    exit: Option<i32>,
}

pub struct BgRegistry {
    jobs: Mutex<HashMap<String, BgJob>>,
    spill_dir: PathBuf,
    counter: std::sync::atomic::AtomicU32,
}

impl BgRegistry {
    pub fn new(spill_dir: impl AsRef<std::path::Path>) -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            spill_dir: spill_dir.as_ref().to_path_buf(),
            counter: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Start a background command; returns the job id.
    pub fn start(
        &self,
        command: &str,
        cwd: &std::path::Path,
    ) -> Result<String, String> {
        let id = {
            let c = self.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!("bg{c}")
        };
        let _ = std::fs::create_dir_all(&self.spill_dir);
        let mut child = {
            let mut builder = Command::new("sh");
            builder
                .arg("-c")
                .arg(command)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            // own process group so bg_kill can reap grandchildren too
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                builder.process_group(0);
            }
            builder.spawn().map_err(|e| format!("spawn: {e}"))?
        };

        let mut buf = JobBuf {
            out: Mutex::new(String::new()),
            err: Mutex::new(String::new()),
            spill_out: None,
            spill_err: None,
        };
        if let Ok(f) = std::fs::File::create(self.spill_dir.join(format!("{id}.out"))) {
            buf.spill_out = Some(Mutex::new(f));
        }
        if let Ok(f) = std::fs::File::create(self.spill_dir.join(format!("{id}.err"))) {
            buf.spill_err = Some(Mutex::new(f));
        }
        let buf = Arc::new(buf);

        let so = child.stdout.take();
        let se = child.stderr.take();
        let b = buf.clone();
        std::thread::spawn(move || {
            if let Some(mut so) = so {
                drain(&mut so, &b, true);
            }
        });
        let b2 = buf.clone();
        std::thread::spawn(move || {
            if let Some(mut se) = se {
                drain(&mut se, &b2, false);
            }
        });

        let job = BgJob {
            id: id.clone(),
            command: command.to_string(),
            child: Some(child),
            buf,
            done: false,
            exit: None,
        };
        self.jobs.lock().unwrap().insert(id.clone(), job);
        Ok(id)
    }

    /// Tail of a job's output since `offset` chars + status.
    pub fn snapshot(&self, id: &str, offset: usize) -> Option<Value> {
        self.reap();
        let g = self.jobs.lock().unwrap();
        let j = g.get(id)?;
        let out = j.buf.out.lock().unwrap();
        let out_len = out.chars().count();
        let tail: String = out.chars().skip(offset).take(20_000).collect();
        Some(json!({
            "id": id,
            "command": j.command,
            "done": j.done,
            "exit": j.exit,
            "output": tail,
            "out_chars": out_len,
        }))
    }

    /// Reap finished children (updates done/exit in place).
    pub fn reap(&self) {
        let mut g = self.jobs.lock().unwrap();
        for j in g.values_mut() {
            if !j.done {
                if let Some(c) = j.child.as_mut() {
                    if let Ok(Some(status)) = c.try_wait() {
                        j.done = true;
                        j.exit = status.code();
                        j.child = None;
                    }
                }
            }
        }
        // bound accumulation: drop the oldest completed jobs (audit MINOR)
        if g.len() > 32 {
            let mut done_ids: Vec<String> = g
                .iter()
                .filter(|(_, j)| j.done)
                .map(|(id, _)| id.clone())
                .collect();
            done_ids.sort();
            let extra = g.len() - 32;
            for id in done_ids.into_iter().take(extra) {
                g.remove(&id);
            }
        }
    }

    pub fn kill(&self, id: &str) -> bool {
        let mut g = self.jobs.lock().unwrap();
        if let Some(j) = g.get_mut(id) {
            if let Some(mut c) = j.child.take() {
                let _ = c.kill();
                // reap the whole process group so grandchildren die too
                #[cfg(unix)]
                {
                    let pgid = c.id() as i32;
                    let _ = std::process::Command::new("kill")
                        .args(["-KILL", &format!("-{pgid}")])
                        .status();
                    let _ = c.wait(); // no zombie
                }
                j.done = true;
                j.exit = Some(-1);
                return true;
            }
        }
        false
    }

    pub fn list(&self) -> Vec<Value> {
        self.reap();
        let g = self.jobs.lock().unwrap();
        g.values()
            .map(|j| {
                json!({
                    "id": j.id,
                    "command": j.command,
                    "done": j.done,
                    "exit": j.exit
                })
            })
            .collect()
    }
}

fn drain<R: std::io::Read>(rd: &mut R, b: &JobBuf, is_out: bool) {
    let mut chunk = [0u8; 8192];
    loop {
        match rd.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let s = String::from_utf8_lossy(&chunk[..n]).into_owned();
                if is_out {
                    b.push_out(&s);
                } else {
                    b.push_err(&s);
                }
            }
            Err(_) => break,
        }
    }
}

/// `bg_bash`: start a command in the background.
pub struct BgBash {
    pub reg: Arc<BgRegistry>,
    pub cwd: PathBuf,
}

impl Tool for BgBash {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bg_bash".into(),
            description: "Run a shell command in the BACKGROUND; returns a job id immediately.
Poll with bg_read, stop with bg_kill, inspect with bg_list. Use for long-running
builds, watchers, servers.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "command":{"type":"string","description":"shell command"}
                },
                "required":["command"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let cmd = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c.to_string(),
            _ => return ToolResult::err("bg_bash: missing 'command'"),
        };
        match self.reg.start(&cmd, &self.cwd) {
            Ok(id) => ToolResult::ok(format!(
                "{id}: started `{cmd}` (poll: bg_read {{\"id\":\"{id}\"}})"
            )),
            Err(e) => ToolResult::err(format!("bg_bash: {e}")),
        }
    }
}

/// `bg_read`: poll a background job's output + status.
pub struct BgRead {
    pub reg: Arc<BgRegistry>,
}

impl Tool for BgRead {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bg_read".into(),
            description: "Read the output + status of a background job started by bg_bash.
Pass {id, offset?} to tail new output since the given char offset.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string","description":"job id from bg_bash"},
                    "offset":{"type":"number","description":"char offset to resume from"}
                },
                "required":["id"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let id = match args.get("id").and_then(|v| v.as_str()) {
            Some(i) => i.to_string(),
            None => return ToolResult::err("bg_read: missing 'id'"),
        };
        let off = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        match self.reg.snapshot(&id, off) {
            Some(v) => ToolResult::ok(v.to_string()),
            None => ToolResult::err(format!("bg_read: unknown job '{id}'")),
        }
    }
}

/// `bg_kill`: stop a background job.
pub struct BgKill {
    pub reg: Arc<BgRegistry>,
}

impl Tool for BgKill {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bg_kill".into(),
            description: "Kill a background job by id (from bg_bash).".into(),
            parameters: json!({
                "type":"object",
                "properties":{"id":{"type":"string","description":"job id"}},
                "required":["id"]
            }),
        }
    }

    fn run(&self, args: Value) -> ToolResult {
        let id = match args.get("id").and_then(|v| v.as_str()) {
            Some(i) => i.to_string(),
            None => return ToolResult::err("bg_kill: missing 'id'"),
        };
        if self.reg.kill(&id) {
            ToolResult::ok(format!("{id}: killed"))
        } else {
            ToolResult::err(format!("bg_kill: unknown job '{id}'"))
        }
    }
}

/// `bg_list`: list all background jobs.
pub struct BgList {
    pub reg: Arc<BgRegistry>,
}

impl Tool for BgList {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bg_list".into(),
            description: "List all background jobs with their status.".into(),
            parameters: json!({"type":"object","properties":{}}),
        }
    }

    fn run(&self, _args: Value) -> ToolResult {
        let l = self.reg.list();
        if l.is_empty() {
            ToolResult::ok("(no background jobs)")
        } else {
            ToolResult::ok(serde_json::json!(l).to_string())
        }
    }
}
