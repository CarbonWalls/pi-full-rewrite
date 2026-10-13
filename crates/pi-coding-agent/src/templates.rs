//! Prompt templates: `.pi/templates/<name>.md` with `{{variable}}`
//! placeholders, filled from built-in variables and `--set key=value`.
//!
//! Built-ins: `{{root}}`, `{{cwd}}`, `{{os}}`, `{{model}}`, `{{date}}`,
//! `{{time}}`.

use std::collections::HashMap;
use std::path::Path;

/// Built-in variables for template substitution.
pub fn builtin_vars(root: &Path, model: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("root".into(), root.display().to_string());
    m.insert(
        "cwd".into(),
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );
    m.insert("os".into(), std::env::consts::OS.to_string());
    m.insert("model".into(), model.to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // ISO-ish date using the civil-from-days helper
    let days = now.div_euclid(86400);
    let rem = now.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let (hh, mm) = ((rem / 3600) % 24, (rem / 60) % 60);
    m.insert("date".into(), format!("{y:04}-{mo:02}-{d:02}"));
    m.insert("time".into(), format!("{hh:02}:{mm:02}"));
    m
}

/// Replace `{{key}}` occurrences; unknown keys stay literal.
pub fn render(text: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        match tail.find("}}") {
            Some(end) => {
                let key = tail[..end].trim();
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("{{");
                        out.push_str(key);
                        out.push_str("}}");
                    }
                }
                rest = &tail[end + 2..];
            }
            None => {
                out.push_str(rest);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Load a template by name from `dir/<name>.md`.
pub fn load(dir: &Path, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err("template: invalid name".into());
    }
    let p = dir.join(format!("{name}.md"));
    std::fs::read_to_string(&p).map_err(|e| format!("template '{name}': {e}"))
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_builtin_and_unknows() {
        let root = Path::new("/tmp/x");
        let v = builtin_vars(root, "mock/echo");
        let out = render("model={{model}} root={{root}} date={{date}} gap={{nope}}", &v);
        assert!(out.contains("model=mock/echo"), "{out}");
        assert!(out.contains("root=/tmp/x"));
        assert!(out.contains("{{nope}}"), "unknown keys stay literal: {out}");
        assert!(out.contains("date=20"), "date present: {out}");
    }

    #[test]
    fn rejects_bad_names() {
        assert!(load(Path::new("."), "../etc").is_err());
        assert!(load(Path::new("."), "a/b").is_err());
        assert!(load(Path::new("."), "").is_err());
    }
}
