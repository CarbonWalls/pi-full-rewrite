//! A multi-line text editor widget (the prompt input box).

use crossterm::event::KeyCode;

use crate::buffer::{Buffer, Color};
use crate::widget::{Area, Widget};
use crate::tui::KeyEvent;

pub struct Editor {
    pub lines: Vec<String>,
    pub cursor: (usize, usize), // (line, col)
    pub history: Vec<String>,
    pub width: usize,
    pub placeholder: String,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: (0, 0),
            history: Vec::new(),
            width: 0,
            placeholder: String::new(),
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.lines.join("").trim().is_empty()
    }

    fn line_mut(&mut self) -> &mut String {
        let l = self.cursor.0.min(self.lines.len() - 1);
        &mut self.lines[l]
    }

    fn line_len(&self) -> usize {
        self.lines.get(self.cursor.0).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// Commit current input to history and clear.
    pub fn commit(&mut self) -> Option<String> {
        let t = self.text();
        if t.trim().is_empty() {
            return None;
        }
        self.history.push(t.clone());
        *self = Self::new();
        Some(t)
    }

    /// Replace the buffer with a single line (used by scripted/paste input).
    pub fn set_line(&mut self, text: String) {
        self.lines = vec![text];
        self.cursor = (0, self.lines[0].chars().count());
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let cur = self.history.len().saturating_sub(1);
        // simple linear walk
        self.lines = vec![self.history[cur].clone()];
        self.cursor = (0, self.history[cur].chars().count());
    }

    /// Insert a hard newline (mapped from Shift+Enter by the caller).
    pub fn newline(&mut self) {
        let l = self.line_mut().clone();
        let col = self.cursor.1.min(l.chars().count());
        let pos = char_offset(&l, col);
        let before: String = l[..pos].to_string();
        let after: String = l[pos..].to_string();
        *self.line_mut() = before;
        self.lines.insert(self.cursor.0 + 1, after);
        self.cursor.0 += 1;
        self.cursor.1 = 0;
    }

    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c) => {
                let mut l = self.line_mut().clone();
                let col = self.cursor.1.min(l.chars().count());
                let pos = char_offset(&l, col);
                l.insert(pos, c);
                *self.line_mut() = l;
                self.cursor.1 += 1;
                true
            }
            KeyCode::Backspace => {
                let l = self.line_mut().clone();
                let col = self.cursor.1.min(l.chars().count());
                if col > 0 {
                    let mut l = self.line_mut().clone();
                    let pos = char_offset(&l, col - 1);
                    let len = l[pos..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(1);
                    l.replace_range(pos..pos + len, "");
                    *self.line_mut() = l;
                    self.cursor.1 -= 1;
                } else if self.cursor.0 > 0 {
                    // join with previous line
                    self.cursor.0 -= 1;
                    let prev_len = self.line_len();
                    self.lines.remove(self.cursor.0 + 1);
                    self.cursor.1 = prev_len;
                }
                true
            }
            KeyCode::Delete => {
                let mut l = self.line_mut().clone();
                let col = self.cursor.1.min(l.chars().count());
                if col < l.chars().count() {
                    let pos = char_offset(&l, col);
                    let len = l[pos..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(1);
                    l.replace_range(pos..pos + len, "");
                    *self.line_mut() = l;
                } else if self.cursor.0 + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cursor.0 + 1);
                    let mut l = self.line_mut().clone();
                    l.push_str(&next);
                    *self.line_mut() = l;
                }
                true
            }
            KeyCode::Left => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                } else if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                    self.cursor.1 = self.line_len();
                }
                true
            }
            KeyCode::Right => {
                let ml = self.line_len();
                if self.cursor.1 < ml {
                    self.cursor.1 += 1;
                } else if self.cursor.0 + 1 < self.lines.len() {
                    self.cursor.0 += 1;
                    self.cursor.1 = 0;
                }
                true
            }
            KeyCode::Up => {
                if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                    self.cursor.1 = self.line_len().min(self.cursor.1);
                } else {
                    self.history_prev();
                }
                true
            }
            KeyCode::Down => {
                if self.cursor.0 + 1 < self.lines.len() {
                    self.cursor.0 += 1;
                    self.cursor.1 = self.line_len().min(self.cursor.1);
                }
                true
            }
            KeyCode::Home => {
                self.cursor.1 = 0;
                true
            }
            KeyCode::End => {
                self.cursor.1 = self.line_len();
                true
            }
            KeyCode::Enter => {
                if self.lines.len() == 1 {
                    // single-line Enter = commit; caller checks this
                    return false;
                }
                self.newline();
                true
            }
            _ => false,
        }
    }
}

fn char_offset(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

impl Widget for Editor {
    fn height(&self, _w: usize) -> usize {
        self.lines.len().max(1)
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area) {
        self.width = area.w;
        let default = Color::default();
        for (i, line) in self.lines.iter().enumerate() {
            let y = area.y + i;
            if y >= area.y + area.h || y >= buf.height {
                break;
            }
            if line.is_empty() && i == 0 {
                buf.print(area.x, y, &self.placeholder, &Color::default().dim(true));
            } else {
                buf.print(area.x, y, line, &default);
            }
        }
        // cursor
        let cl = self.cursor.0;
        let cy = area.y + cl;
        if cy < buf.height && cy >= area.y {
            let line = self.lines.get(cl).map(|s| s.as_str()).unwrap_or("");
            // clamp column to visible width
            let visible = line.chars().take(area.w).count();
            let cx = area.x + self.cursor.1.min(visible);
            buf.set_cursor(cx, cy);
        }
    }
    fn on_event(&mut self, ev: &KeyEvent) {
        match &ev.key {
            crate::tui::Key::Code(c) => {
                let _ = self.handle_key(*c);
            }
            crate::tui::Key::Paste(s) => {
                let mut l = self.line_mut().clone();
                let col = self.cursor.1.min(l.chars().count());
                let pos = char_offset(&l, col);
                l.insert_str(pos, s);
                *self.line_mut() = l;
                self.cursor.1 = col + s.chars().count();
            }
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
