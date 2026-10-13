//! Project config: `<root>/.pi/pi.json` supplies defaults that the CLI
//! overrides. Also `pi --init` scaffolds the project directory.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    pub model: Option<String>,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub thinking: Option<String>,
    pub skills: Option<String>,
    pub theme: Option<String>,
    pub compact_threshold: Option<f64>,
    pub session_dir: Option<String>,
}

impl FileConfig {
    pub fn load(root: &Path) -> Result<FileConfig, String> {
        let p = root.join(".pi").join("pi.json");
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(_) => return Ok(FileConfig::default()),
        };
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Scaffold `.pi/` with a template config, skills and sessions dirs.
    pub fn init(root: &Path) -> anyhow::Result<()> {
        let pidir = root.join(".pi");
        std::fs::create_dir_all(pidir.join("skills"))?;
        std::fs::create_dir_all(pidir.join("sessions"))?;
        let mut readme = pidir.join("skills");
        let _ = &mut readme;
        let _ = std::fs::write(pidir.join("skills").join("README.md"), "# skills\n\nDrop `name.md` files here and load them with `pi --skills name`.\nFront matter: `---\\nname: <skill>\\ndescription: <one line>\\n---`\n");
        let tmpl = r#"{
  "//comment": "pi project config. CLI flags override these defaults.",
  "model": "openai/gpt-4o-mini",
  "max_turns": 50,
  "compact_threshold": 0.85,
  "skills": null,
  "system": null,
  "temperature": null,
  "thinking": null,
  "theme": null,
  "session_dir": null
}
"#;
        let cfg_path = pidir.join("pi.json");
        if !cfg_path.exists() {
            std::fs::write(&cfg_path, tmpl)?;
        }
        eprintln!(
            "pi init: wrote {} (+ skills/ and sessions/)",
            cfg_path.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_fields_and_defaults() {
        let dir = std::env::temp_dir().join(format!("pi-cfg-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".pi")).unwrap();
        std::fs::write(
            dir.join(".pi/pi.json"),
            r#"{
              "model": "openai/gpt-4o",
              "system": "be concise",
              "temperature": 0.2,
              "max_turns": 7,
              "thinking": "low",
              "skills": "my-skills/",
              "theme": "examples/one-dark.json",
              "compact_threshold": 0.5,
              "session_dir": ".pi/custom",
              "unknown_field": 42
            }"#,
        )
        .unwrap();
        let c = FileConfig::load(&dir).unwrap();
        assert_eq!(c.model.as_deref(), Some("openai/gpt-4o"));
        assert_eq!(c.system.as_deref(), Some("be concise"));
        assert_eq!(c.temperature, Some(0.2));
        assert_eq!(c.max_turns, Some(7));
        assert_eq!(c.thinking.as_deref(), Some("low"));
        assert_eq!(c.skills.as_deref(), Some("my-skills/"));
        assert_eq!(c.compact_threshold, Some(0.5));
        assert_eq!(c.session_dir.as_deref(), Some(".pi/custom"));
        // unknown fields are ignored (serde deny_unknown not set)
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absent_config_is_defaults() {
        let dir = std::env::temp_dir().join(format!("pi-cfg2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = FileConfig::load(&dir).unwrap();
        assert_eq!(c.model, None);
        assert_eq!(c.temperature, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_config_errors() {
        let dir = std::env::temp_dir().join(format!("pi-cfg3-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".pi")).unwrap();
        std::fs::write(dir.join(".pi/pi.json"), "{ not json").unwrap();
        let err = FileConfig::load(&dir).unwrap_err();
        assert!(err.contains("pi.json"), "error names the file: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
