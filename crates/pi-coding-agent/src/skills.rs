//! Skills: reusable instruction sets loaded from `<root>/.pi/skills/*.md`
//! (`--skills name1,name2` or `--skills all`).
//!
//! A skill file:
//! ```markdown
//! ---
//! name: rust
//! description: Rust workspace conventions
//! ---
//! Body of instructions appended to the system prompt...
//! ```

use std::path::{Path, PathBuf};

pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// Discover skills in `dir` (missing dir = empty set).
pub fn discover(dir: &Path) -> Vec<Skill> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "md").unwrap_or(false))
        .collect();
    files.sort();
    for f in files {
        if let Some(s) = parse_file(&f) {
            out.push(s);
        }
    }
    out
}

fn parse_file(path: &Path) -> Option<Skill> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut description = String::new();
    let mut body = text.as_str();

    // optional front matter: --- lines at the top
    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("---") {
            let fm = &rest[..end];
            body = &rest[end + 3..];
            for line in fm.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    match k.trim() {
                        "name" => name = v.to_string(),
                        "description" => description = v.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }
    if name.is_empty() {
        return None;
    }
    Some(Skill {
        name,
        description,
        body: body.trim().to_string(),
    })
}

/// Select skills by comma-separated names; `all` selects every skill.
/// Returns the system-prompt fragment for the selected skills.
pub fn render_selected(dir: &Path, selection: &str) -> Result<String, String> {
    let skills = discover(dir);
    if selection.trim().is_empty() {
        return Ok(String::new());
    }
    let want_all = selection.split(',').any(|s| s.trim() == "all");
    let wanted: Vec<String> = selection
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "all")
        .collect();
    let mut out = String::new();
    for s in &skills {
        let selected = want_all || wanted.iter().any(|w| w == &s.name);
        if !selected {
            continue;
        }
        let desc = if s.description.is_empty() {
            String::new()
        } else {
            format!(" ({})", s.description)
        };
        out.push_str(&format!("\n## Skill: {}{}\n{}\n", s.name, desc, s.body));
    }
    // Report names that were requested but not found.
    for w in &wanted {
        if !skills.iter().any(|s| s.name == *w) {
            return Err(format!("skill '{w}' not found in {}", dir.display()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "pi-skills-test-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64
    }

    #[test]
    fn parses_frontmatter_and_selects() {
        let d = tmp();
        std::fs::write(
            d.join("rust.md"),
            "---\nname: rust\ndescription: Rust conventions\n---\nAlways run `cargo test`.\n",
        )
        .unwrap();
        std::fs::write(d.join("go.md"), "go skill body\n").unwrap();
        let out = render_selected(&d, "rust").unwrap();
        assert!(out.contains("Skill: rust"), "{out}");
        assert!(out.contains("cargo test"));
        assert!(!out.contains("go skill"));
        let all = render_selected(&d, "all").unwrap();
        assert!(all.contains("go skill"));
        assert!(render_selected(&d, "nope").is_err());
        let _ = std::fs::remove_dir_all(d);
    }
}
