//! JSON-overridable color themes for the TUI.
//!
//! A theme is a map of named colors; load from a JSON file (`--theme`), the
//! `PI_THEME` env (path or inline JSON), or fall back to the built-in
//! palette. Unknown keys are ignored so themes stay forward-compatible.

use crate::buffer::Color;
use std::path::Path;

fn c_fg(r: u8, g: u8, b: u8) -> Color {
    Color::default().fg_rgb(r, g, b)
}

/// Named colors used by the TUI.
#[derive(Clone, Debug)]
pub struct Theme {
    pub h1: Color,
    pub h2: Color,
    pub bold: Color,
    pub code: Color,
    pub code_bg: Color,
    pub link: Color,
    pub dim: Color,
    pub body: Color,
    pub user: Color,
    pub assistant: Color,
    pub tool: Color,
    pub error: Color,
    pub spinner: Color,
    pub prompt: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            h1: c_fg(220, 50, 47),
            h2: c_fg(163, 113, 242),
            bold: c_fg(229, 229, 229),
            code: c_fg(249, 115, 22),
            code_bg: c_fg(249, 115, 22),
            link: c_fg(56, 189, 248),
            dim: c_fg(113, 113, 113),
            body: c_fg(213, 213, 213),
            user: c_fg(94, 129, 172),
            assistant: c_fg(213, 213, 213),
            tool: c_fg(137, 180, 50),
            error: c_fg(224, 108, 117),
            spinner: c_fg(137, 180, 50),
            prompt: c_fg(229, 192, 50),
        }
    }
}

impl Theme {
    /// Parse a color spec: `"#rrggbb"`, `"rgb(r,g,b)"`, `[r,g,b]`, or
    /// `"default"`.
    fn parse_color(v: &serde_json::Value) -> Option<Color> {
        match v {
            serde_json::Value::String(s) => {
                let s = s.trim();
                if s == "default" {
                    return Some(Color::default());
                }
                if let Some(hex) = s.strip_prefix('#') {
                    if hex.len() == 6 {
                        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                        return Some(c_fg(r, g, b));
                    }
                }
                if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() == 3 {
                        let r: u8 = parts[0].trim().parse().ok()?;
                        let g: u8 = parts[1].trim().parse().ok()?;
                        let b: u8 = parts[2].trim().parse().ok()?;
                        return Some(c_fg(r, g, b));
                    }
                }
                None
            }
            serde_json::Value::Array(a) => {
                if a.len() != 3 {
                    return None;
                }
                let r = a[0].as_u64()? as u8;
                let g = a[1].as_u64()? as u8;
                let b = a[2].as_u64()? as u8;
                Some(c_fg(r, g, b))
            }
            _ => None,
        }
    }

    /// Load a theme from JSON text (a map of name -> color, or
    /// `{"colors": {...}}`). Missing names keep defaults.
    pub fn from_json(text: &str) -> Result<Theme, String> {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("theme json: {e}"))?;
        // If a top-level "colors" key exists it must be an object; otherwise
        // treat the whole document as the color map.
        let colors = match v.get("colors") {
            Some(c) => c,
            None => &v,
        };
        let map = match colors.as_object() {
            Some(m) => m,
            None => return Err("theme: expected a color map".into()),
        };
        let mut t = Theme::default();
        let assign = |name: &str, dst: &mut Color| {
            if let Some(cv) = map.get(name) {
                if let Some(c) = Self::parse_color(cv) {
                    *dst = c;
                }
            }
        };
        assign("h1", &mut t.h1);
        assign("h2", &mut t.h2);
        assign("bold", &mut t.bold);
        assign("code", &mut t.code);
        assign("code_bg", &mut t.code_bg);
        assign("link", &mut t.link);
        assign("dim", &mut t.dim);
        assign("body", &mut t.body);
        assign("user", &mut t.user);
        assign("assistant", &mut t.assistant);
        assign("tool", &mut t.tool);
        assign("error", &mut t.error);
        assign("spinner", &mut t.spinner);
        assign("prompt", &mut t.prompt);
        Ok(t)
    }

    /// Load from a file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Theme, String> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read theme {}: {e}", path.as_ref().display()))?;
        Self::from_json(&text)
    }

    /// Resolve: inline JSON if `spec` parses as JSON, else treat as a file
    /// path; fall back to the env var `PI_THEME`; else default.
    pub fn resolve(spec: Option<&str>) -> Theme {
        if let Some(s) = spec {
            let s = s.trim();
            if let Ok(t) = Theme::from_json(s) {
                return t;
            }
            if !s.is_empty() {
                if let Ok(t) = Theme::from_file(s) {
                    return t;
                }
            }
        }
        if let Ok(s) = std::env::var("PI_THEME") {
            let s = s.trim();
            if !s.is_empty() {
                if let Ok(t) = Theme::from_json(s) {
                    return t;
                }
                if let Ok(t) = Theme::from_file(s) {
                    return t;
                }
            }
        }
        Theme::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_arrays() {
        let t = Theme::from_json(
            r##"{"colors":{"h1":"#ff0000","code":[0,128,255],"body":"default"}}"##,
        )
        .unwrap();
        assert_eq!(t.h1.fg, 0x0100_0000 | 0x00ff_0000);
        assert_eq!(t.code.fg, 0x0100_0000 | 0x0000_80ff);
        assert_eq!(t.body, Color::default(), "\"default\" resets to terminal default");
        assert_eq!(t.h2, Theme::default().h2, "unlisted keys keep defaults");
    }

    #[test]
    fn accepts_rgb_strings_and_bad_input() {
        let t = Theme::from_json(r#"{"link":"rgb(1,2,3)"}"#).unwrap();
        assert_eq!(t.link.fg, 0x0100_0000 | 0x0001_0203);
        assert!(Theme::from_json("not json").is_err());
        assert!(Theme::from_json(r#"{"colors":"x"}"#).is_err());
    }
}
