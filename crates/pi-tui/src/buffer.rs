//! Cell-level differential terminal renderer.
//!
//! We keep a *previous* and a *current* grid of [`Cell`]s and emit only the
//! minimal ANSI needed to converge (runs of identical styled text, background
//! fills, cursor move). This is the core of pi's `pi-tui` "differential
//! rendering" re-implemented from scratch and dependency-light.

use std::fmt::Write;

/// 256/true-color packed into u32: 0 = default, else 0xRRGGBB | 0x0100_0000.
const TRUE_COLOR: u32 = 0x0100_0000;
const M_BOLD: u8 = 1 << 0;
const M_DIM: u8 = 1 << 1;
const M_ITALIC: u8 = 1 << 2;
const M_INVERT: u8 = 1 << 3;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Color {
    /// packed: 0 default, TRUE_COLOR flag + rgb
    pub fg: u32,
    pub bg: u32,
    pub dim: bool,
    pub bold: bool,
    pub italic: bool,
    pub invert: bool,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
        TRUE_COLOR | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
    }
    pub fn fg_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.fg = Self::rgb(r, g, b);
        self
    }
    pub fn bg_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.bg = Self::rgb(r, g, b);
        self
    }
    pub fn dim(mut self, v: bool) -> Self {
        self.dim = v;
        self
    }
    pub fn bold(mut self, v: bool) -> Self {
        self.bold = v;
        self
    }
}

/// One terminal cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cell {
    /// UTF-8 code point to draw (0 = blank).
    pub ch: char,
    pub style: Color,
}

/// A frame's grid.
#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    /// cursor row/col within this buffer (None = hidden)
    pub cursor: Option<(usize, usize)>,
}

impl Buffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
            cursor: None,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if width == self.width && height == self.height {
            return;
        }
        let mut cells = vec![Cell::default(); width * height];
        // copy the overlap
        for y in 0..self.height.min(height) {
            for x in 0..self.width.min(width) {
                cells[y * width + x] = self.cells[y * self.width + x];
            }
        }
        self.cells = cells;
        self.width = width;
        self.height = height;
        self.cursor = None;
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
    }
    #[inline]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        &mut self.cells[y * self.width + x]
    }

    /// Write a styled string starting at (x,y), clipping to the buffer.
    pub fn print(&mut self, mut x: usize, y: usize, s: &str, style: &Color) {
        if y >= self.height {
            return;
        }
        for ch in s.chars() {
            if x >= self.width {
                break;
            }
            let c = self.get_mut(x, y);
            c.ch = ch;
            c.style = *style;
            // wide chars (CJK/emoji) take two terminal columns: the buffer
            // must also skip the second column so absolute CUP positioning
            // in diff() stays aligned with what the terminal displays.
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            if w > 1 && x + 1 < self.width {
                // the wide char's second terminal column: blank so the
                // buffer models the terminal's column grid exactly.
                let n = self.get_mut(x + 1, y);
                n.ch = ' ';
                n.style = *style;
            }
            x += w.max(1);
        }
    }

    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.cursor = Some((x, y));
    }
    pub fn hide_cursor(&mut self) {
        self.cursor = None;
    }
}

/// Compute the ANSI to render `cur` given `prev` was on screen.
pub struct Diff {
    pub out: String,
}

pub fn diff(prev: &Buffer, cur: &Buffer, out: &mut String) {
    // If size changed, we must reset (clear + home) because coordinates shift.
    if prev.width != cur.width || prev.height != cur.height {
        clear_all(out);
        return;
    }

    let mut last_fg: u32 = 0;
    let mut last_bg: u32 = 0;
    let mut last_mod: u8 = 0;

    for y in 0..cur.height {
        for x in 0..cur.width {
            let c = cur.get(x, y);
            let p = prev.get(x, y);
            if c == p {
                continue; // nothing changed
            }

            // position (ANSI CUP is 1-based; no trailing newline)
            write!(out, "\x1b[{};{}H", y + 1, x + 1).unwrap();

            // style
            let mut mod_now: u8 = 0;
            if c.style.bold { mod_now |= M_BOLD; }
            if c.style.dim { mod_now |= M_DIM; }
            if c.style.italic { mod_now |= M_ITALIC; }
            if c.style.invert { mod_now |= M_INVERT; }

            let need_fg = c.style.fg != last_fg;
            let need_bg = c.style.bg != last_bg;
            let need_mod = mod_now != last_mod;

            if need_fg || need_bg || need_mod {
                out.push_str("\x1b[0m");
                if need_mod {
                    if mod_now & M_BOLD != 0 { out.push_str("\x1b[1m"); }
                    if mod_now & M_DIM != 0 { out.push_str("\x1b[2m"); }
                    if mod_now & M_ITALIC != 0 { out.push_str("\x1b[3m"); }
                    if mod_now & M_INVERT != 0 { out.push_str("\x1b[7m"); }
                }
                if need_fg {
                    push_color(out, c.style.fg);
                }
                if need_bg {
                    push_bg(out, c.style.bg);
                }
                last_fg = c.style.fg;
                last_bg = c.style.bg;
                last_mod = mod_now;
            }

            // glyph
            if c.ch == '\0' {
                out.push(' ');
            } else {
                out.push(c.ch);
            }
        }
    }

    // place the cursor (1-based; no trailing newline)
    match cur.cursor {
        Some((x, y)) => {
            write!(out, "\x1b[{};{}H", y + 1, x + 1).unwrap();
            out.push_str("\x1b[?25h"); // show
        }
        None => out.push_str("\x1b[?25l"), // hide
    }
}

fn push_color(out: &mut String, v: u32) {
    if v == 0 {
        out.push_str("\x1b[39m"); // default fg
    } else if v & TRUE_COLOR != 0 {
        let r = ((v >> 16) & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = (v & 0xFF) as u8;
        write!(out, "\x1b[38;2;{r};{g};{b}m").unwrap();
    } else {
        write!(out, "\x1b[38;5;{v}m").unwrap();
    }
}

fn push_bg(out: &mut String, v: u32) {
    if v == 0 {
        out.push_str("\x1b[49m");
    } else if v & TRUE_COLOR != 0 {
        let r = ((v >> 16) & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = (v & 0xFF) as u8;
        write!(out, "\x1b[48;2;{r};{g};{b}m").unwrap();
    } else {
        write!(out, "\x1b[48;5;{v}m").unwrap();
    }
}

fn clear_all(out: &mut String) {
    out.push_str("\x1b[2J\x1b[H");
}

/// Build a terminal string from a buffer (for tests / non-interactive dumps).
pub fn render_to_string(buf: &Buffer) -> String {
    let mut s = String::new();
    for y in 0..buf.height {
        for x in 0..buf.width {
            let c = buf.get(x, y);
            if c.ch == '\0' {
                s.push(' ');
            } else {
                s.push(c.ch);
            }
        }
        if y + 1 < buf.height {
            s.push('\n');
        }
    }
    s
}

