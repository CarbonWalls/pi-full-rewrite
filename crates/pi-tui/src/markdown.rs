//! Lightweight markdown -> styled lines. Supports headings, **bold**,
//! *italic*, `code`, ``` fences, lists, and links. Enough for agent output.

use crate::buffer::Color;
use crate::theme::Theme;

/// A styled run of text within one line.
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub text: String,
    pub style: Color,
}

#[derive(Clone, Debug, Default)]
pub struct StyledLine {
    pub runs: Vec<Run>,
}

impl StyledLine {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}


fn style_for(kind: &str, color: Color, bold: bool) -> Color {
    let mut c = color;
    let _ = kind;
    c.bold = bold;
    c
}

/// Render markdown into styled lines with the default theme.
pub fn render(md: &str) -> Vec<StyledLine> {
    render_with(md, &Theme::default())
}

/// Render markdown into styled lines using `theme` (one line per source line;
/// code fences toggle styling). No wrapping (the caller wraps against width).
pub fn render_with(md: &str, theme: &Theme) -> Vec<StyledLine> {
    let h1 = theme.h1;
    let h2 = theme.h2;
    let bold = theme.bold;
    let code = theme.code;
    let code_bg = theme.code_bg;
    let link = theme.link;
    let dim = theme.dim;
    let body_c = theme.body;
    let mut out = Vec::new();
    let mut in_code = false;
    for line in md.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            out.push(StyledLine {
                runs: vec![Run {
                    text: line.to_string(),
                    style: code_bg.dim(true),
                }],
            });
            continue;
        }
        if in_code {
            out.push(StyledLine {
                runs: vec![Run {
                    text: line.to_string(),
                    style: code.dim(true),
                }],
            });
            continue;
        }

        // headings
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push(StyledLine {
                runs: vec![Run {
                    text: rest.to_string(),
                    style: style_for("h3", h2, true),
                }],
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push(StyledLine {
                runs: vec![Run {
                    text: rest.to_string(),
                    style: style_for("h2", h2, true),
                }],
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push(StyledLine {
                runs: vec![Run {
                    text: rest.to_string(),
                    style: style_for("h1", h1, true),
                }],
            });
            continue;
        }
        // list bullets
        let list_prefix = if trimmed.starts_with("- ") || trimmed.starts_with('*') {
            "  "
        } else {
            ""
        };

        // inline parse: **bold**, *italic*, `code`, [text](url)
        let mut runs = Vec::new();
        if !list_prefix.is_empty() {
            runs.push(Run {
                text: list_prefix.to_string(),
                style: body_c,
            });
        }
        let body = trimmed
            .strip_prefix("- ")
            .map(|s| s.to_string())
            .unwrap_or_else(|| line.trim_start().trim_end().to_string());
        parse_inline(body, &mut runs, body_c, code, link, dim, bold);
        out.push(StyledLine { runs });
    }
    out
}

fn parse_inline(
    s: String,
    runs: &mut Vec<Run>,
    body_c: Color,
    code: Color,
    link: Color,
    dim: Color,
    bold: Color,
) {
    let mut i = 0usize;
    let bytes = s.as_bytes();
    let mut plain = String::new();
    let flush_plain = |plain: &String, runs: &mut Vec<Run>| {
        if plain.is_empty() {
            return;
        }
        let t = plain.clone();
        runs.push(Run {
            text: t,
            style: body_c,
        });
    };
    while i < bytes.len() {
        let ch = s[i..].chars().next().unwrap();
        if ch == '`' {
            flush_plain(&plain, runs);
            plain.clear();
            if let Some(end) = s[i + 1..].find('`') {
                let inner = &s[i + 1..i + 1 + end];
                runs.push(Run {
                    text: inner.to_string(),
                    style: code,
                });
                i += end + 2;
                continue;
            }
        } else if ch == '*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            flush_plain(&plain, runs);
            plain.clear();
            if let Some(end) = s[i + 2..].find("**") {
                let inner = &s[i + 2..i + 2 + end];
                runs.push(Run {
                    text: inner.to_string(),
                    style: style_for("b", bold, true),
                });
                i += end + 4;
                continue;
            }
            i += 1;
            continue;
        } else if ch == '*' {
            flush_plain(&plain, runs);
            plain.clear();
            if let Some(end) = s[i + 1..].find('*') {
                let inner = &s[i + 1..i + 1 + end];
                let mut st = dim;
                st.italic = true;
                runs.push(Run {
                    text: inner.to_string(),
                    style: st,
                });
                i += end + 3;
                continue;
            }
            i += 1;
            continue;
        } else if ch == '[' {
            flush_plain(&plain, runs);
            plain.clear();
            if let Some(close) = s[i..].find(']') {
                let text = &s[i + 1..i + close];
                let after = &s[i + close + 1..];
                if let Some(open) = after.find('(') {
                    if let Some(url_end) = after[open + 1..].find(')') {
                        let url = &after[open + 1..open + 1 + url_end];
                        runs.push(Run {
                            text: format!("[{text}]({url})"),
                            style: link,
                        });
                        i += close + open + url_end + 2;
                        continue;
                    }
                }
            }
            i += 1;
            continue;
        }
        plain.push(ch);
        i += ch.len_utf8();
    }
    flush_plain(&plain, runs);
}

#[allow(dead_code)]
fn flush(_text: &str, _style: Color, _runs: &mut Vec<Run>) {}
