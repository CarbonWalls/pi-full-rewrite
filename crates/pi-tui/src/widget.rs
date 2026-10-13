//! Widget trait + containers.

use crate::buffer::{Buffer, Color};

/// A rectangular region in a buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Area {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self { x, y, w, h }
    }
    pub fn intersect(self, other: &Area) -> Option<Area> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        Some(Area::new(x0, y0, x1 - x0, y1 - y0))
    }
    pub fn inner(self, pad: u16) -> Area {
        if self.w as u16 <= 2 * pad || self.h as u16 <= 2 * pad {
            return Area::new(self.x, self.y, 0, 0);
        }
        Area::new(
            self.x + pad as usize,
            self.y + pad as usize,
            self.w - 2 * pad as usize,
            self.h - 2 * pad as usize,
        )
    }
}

/// A renderable widget.
pub trait Widget {
    /// Preferred height when given a width.
    fn height(&self, _w: usize) -> usize {
        1
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area);
    fn on_event(&mut self, _ev: &crate::tui::KeyEvent) {}
}

/// Stack widgets vertically, one after another.
pub struct Column {
    pub widgets: Vec<Box<dyn Widget>>,
}

impl Column {
    pub fn new() -> Self {
        Self { widgets: Vec::new() }
    }
    pub fn with<W: Widget + 'static>(mut self, w: W) -> Self {
        self.widgets.push(Box::new(w));
        self
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Column {
    fn height(&self, w: usize) -> usize {
        self.widgets.iter().map(|c| c.height(w)).sum()
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area) {
        let bottom = area.y + area.h;
        let mut y = area.y;
        for w in self.widgets.iter_mut() {
            if y >= bottom {
                break;
            }
            let h = w.height(area.w).min(bottom - y);
            let sub = Area::new(area.x, y, area.w, h);
            w.render(buf, &sub);
            y += h;
        }
    }
    fn on_event(&mut self, ev: &crate::tui::KeyEvent) {
        // forward to the last widget that wants it (editor at bottom)
        for w in self.widgets.iter_mut().rev() {
            w.on_event(ev);
        }
    }
}

/// A static block of styled lines (for log / transcript view).
pub struct TextBlock {
    pub lines: Vec<Vec<(String, crate::buffer::Color)>>,
}

impl TextBlock {
    pub fn new(text: &str) -> Self {
        Self {
            lines: text
                .lines()
                .map(|l| vec![(l.to_string(), Color::default())])
                .collect(),
        }
    }
}

impl Widget for TextBlock {
    fn height(&self, _w: usize) -> usize {
        self.lines.len().max(1)
    }
    fn render(&mut self, buf: &mut Buffer, area: &Area) {
        for (i, line) in self.lines.iter().enumerate() {
            let y = area.y + i;
            if y >= area.y + area.h || y >= buf.height {
                break;
            }
            for (s, style) in line {
                buf.print(area.x, y, s, style);
            }
        }
    }
}

