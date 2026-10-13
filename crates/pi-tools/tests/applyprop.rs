//! Property test: apply_diff must reproduce any mutation that `git diff`
//! generates, over many deterministic pseudo-random files/mutations.

use pi_agent::tools::ToolRegistry;
use pi_tools::Toolset;
use serde_json::json;
use std::path::PathBuf;

const VOCAB: [&str; 14] = [
    "fn main() {", "let x = 1;", "println!(\"hi\");", "}", "// comment",
    "fn foo(a: i32) -> i32 {", "a + 1", "let mut v = vec![];", "v.push(x);",
    "match n {", "_ => 0,", "hello", "world", "return",
];

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn rand_line(state: &mut u64) -> String {
    let w = VOCAB[(next(state) as usize) % VOCAB.len()];
    let mut s = String::from(w);
    for _ in 0..(next(state) % 3) {
        s.push(' ');
    }
    if next(state).is_multiple_of(2) {
        s.push('\t');
    }
    s
}

fn gen_lines(state: &mut u64) -> Vec<String> {
    let n = (next(state) % 40) as usize + 3;
    (0..n).map(|_| rand_line(state)).collect()
}

fn mutate(state: &mut u64, original: &[String]) -> Vec<String> {
    let mut out: Vec<String> = original.to_vec();
    for _ in 0..(next(state) % 5) {
        let pos = if out.is_empty() {
            0
        } else {
            (next(state) as usize) % out.len()
        };
        match next(state) % 3 {
            0 => {
                if !out.is_empty() {
                    out.remove(pos);
                }
            }
            1 => out.insert(pos, rand_line(state)),
            _ => out[pos] = rand_line(state),
        }
    }
    out
}

fn git_run(dir: &PathBuf, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git")
}

#[test]
fn apply_diff_round_trips_random_mutations() {
    let dir = std::env::temp_dir().join(format!("pi-applyprop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ts = Toolset::new(&dir);
    let mut reg = ToolRegistry::new();
    ts.register_all(&mut reg);
    assert!(git_run(&dir, &["init", "-q"]).status.success());

    for seed in 0..150u64 {
        let mut state = 0x9E3779B97F4A7C15 ^ seed.rotate_left(32) | (seed << 11);
        let original = gen_lines(&mut state);
        let mutated = mutate(&mut state, &original);
        if mutated == original {
            continue;
        }
        let text = |lines: &[String]| lines.join("\n") + "\n";

        std::fs::write(dir.join("f.txt"), text(&original)).unwrap();
        let _ = git_run(&dir, &["add", "f.txt"]);
        if !git_run(&dir, &["commit", "-qm", "base"]).status.success() {
            continue;
        }
        std::fs::write(dir.join("f.txt"), text(&mutated)).unwrap();
        let diff = String::from_utf8_lossy(&git_run(&dir, &["diff"]).stdout).to_string();
        std::fs::write(dir.join("f.txt"), text(&original)).unwrap();

        let r = reg.execute_named("apply_diff", json!({"file": "f.txt", "diff": diff}));
        assert!(!r.is_error, "seed {seed}: apply failed: {}", r.output.chars().take(300).collect::<String>());
        let now = std::fs::read_to_string(dir.join("f.txt")).unwrap();
        assert_eq!(now, text(&mutated), "seed {seed}: mismatch");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
