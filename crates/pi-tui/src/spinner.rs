//! A spinner + status line widget.

use crate::buffer::{Buffer, Color};
use crate::widget::{Area, Widget};

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Default)]
pub struct Spinner {
    pub label: String,
    pub frame: usize,
    pub spin: bool,
}

impl Spinner {
    /// Rendered glyph width of the current status line (frame + label). Used
    /// by the TUI to place the model/session suffix after it.
    pub fn width(&self) -> usize {
        let frame = if self.spin {
            FRAMES[self.frame % FRAMES.len()].to_string()
        } else {
            "•".to_string()
        };
        if frame.is_empty() {
            self.label.chars().count()
        } else {
            frame.chars().count() + 1 + self.label.chars().count()
        }
    }

    pub fn new(label: &str) -> Self {
        Self {
            label: label.into(),
            frame: 0,
            spin: true,
        }
    }
    pub fn tick(&mut self) {
        if self.spin {
            self.frame = (self.frame + 1) % FRAMES.len();
        }
    }
    pub fn set_label(&mut self, s: &str) {
        self.label = s.into();
    }
    pub fn stop(&mut self) {
        self.spin = false;
    }
}

impl Widget for Spinner {
    fn height(&self, _w: usize) -> usize {
        1
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area) {
        let frame = if self.spin {
            FRAMES[self.frame % FRAMES.len()].to_string()
        } else {
            "•".to_string()
        };
        let mut text = if frame.is_empty() {
            self.label.clone()
        } else {
            format!("{frame} {label}", label = self.label)
        };
        // pad / clip to width
        let w = area.w.saturating_sub(1);
        if text.chars().count() > w {
            text = text.chars().take(w).collect();
        }
        let mut c = Color::default().fg_rgb(86, 156, 214);
        c.bold = false;
        buf.print(area.x, area.y, &text, &c);
        buf.hide_cursor();
    }
}
