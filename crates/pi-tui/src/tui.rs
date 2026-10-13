//! TUI driver: raw mode, event poll, differential render loop.

use std::io::Write;
use std::io;
use std::time::Duration;

use crossterm::event;
use crossterm::event::KeyCode;
use crossterm::terminal;
use crossterm::queue;

use crate::buffer::{diff, Buffer, Color};
use crate::widget::Widget;

/// A key event delivered to widgets.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Code(KeyCode),
    Paste(String),
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: u8,
}

/// The terminal driver.
pub struct Tui {
    pub width: u16,
    pub height: u16,
    prev: Buffer,
    cur: Buffer,
    pub alt_screen: bool,
}

impl Tui {
    /// Enter raw mode + hide cursor, capture size.
    pub fn start() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = std::io::stdout();
        let _ = queue!(out, terminal::EnterAlternateScreen);
        let _ = out.write_all(b"\x1b[?2004h\x1b[?25l"); // bracketed paste + hide cursor
        let _ = out.flush();
        let (w, h) = terminal::size()?;
        let cur = Buffer::new(w as usize, h as usize);
        let prev = cur.clone();
        Ok(Self {
            width: w,
            height: h,
            prev,
            cur,
            alt_screen: true,
        })
    }

    /// Leave raw mode, restore the screen.
    pub fn stop(self) -> io::Result<()> {
        let mut out = std::io::stdout();
        let _ = queue!(out, terminal::LeaveAlternateScreen);
        let _ = out.write_all(b"\x1b[?2004l\x1b[?25h\x1b[0m"); // paste off, cursor, reset
        let _ = out.flush();
        terminal::disable_raw_mode()?;
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Poll one event within `timeout` (None = wait forever).
    pub fn poll_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<KeyEvent>> {
        let have = match timeout {
            Some(d) => event::poll(d)?,
            None => true,
        };
        if !have {
            return Ok(None);
        }
        let ev = event::read()?;
        match ev {
            event::Event::Key(k) => {
                self.cur.hide_cursor();
                // In raw mode crossterm reports LF (0x0A) — which is what
                // piped/scripted input delivers for Enter — as Ctrl+J
                // (KeyCode::Char('j') + CONTROL). Real terminals send CR,
                // which already maps to Enter. Map Ctrl+J -> Enter so both
                // interactive and automated input submit.
                let code = if k.code == KeyCode::Char('j')
                    && k.modifiers == crossterm::event::KeyModifiers::CONTROL
                {
                    KeyCode::Enter
                } else {
                    k.code
                };
                Ok(Some(KeyEvent {
                    key: Key::Code(code),
                    mods: k.modifiers.bits(),
                }))
            }
            event::Event::Paste(s) => Ok(Some(KeyEvent {
                key: Key::Paste(s),
                mods: 0,
            })),
            event::Event::Resize(w, h) => {
                self.width = w;
                self.height = h;
                self.cur.resize(w as usize, h as usize);
                Ok(None)
            }
            event::Event::Mouse(_) | event::Event::FocusGained | event::Event::FocusLost => Ok(None),
            // wildcard kept for crossterm forward-compat
            #[allow(unreachable_patterns)]
            _ => Ok(None),
        }
    }

    /// Render a widget tree: build `cur`, diff against `prev`, write ANSI.
    pub fn render(&mut self, widget: &mut dyn Widget) -> io::Result<()> {
        self.cur.resize(self.width as usize, self.height as usize);
        // clear only the region the widget actually covers so unchanged
        // history above stays (efficient re-paint).
        let full = crate::widget::Area::new(0, 0, self.width as usize, self.height as usize);
        widget.render(&mut self.cur, &full);
        let mut out = String::new();
        diff(&self.prev, &self.cur, &mut out);
        let mut so = std::io::stdout().lock();
        so.write_all(out.as_bytes())?;
        so.flush()?;
        self.prev = std::mem::take(&mut self.cur);
        Ok(())
    }

    /// Paint a spinner / status line at the bottom row.
    pub fn draw_status(&mut self, text: &str, color: &Color) -> io::Result<()> {
        let mut buf = Buffer::new(self.width as usize, 1);
        buf.print(0, 0, text, color);
        let mut out = String::new();
        // single-line overlay: just clear the line and print
        out.push_str("\x1b[2K");
        out.push_str(&format!("\x1b[{};1H", self.height));
        out.push_str(&crate::buffer::render_to_string(&buf));
        let mut so = std::io::stdout().lock();
        so.write_all(out.as_bytes())?;
        so.flush()?;
        Ok(())
    }
}

/// Run a widget for a fixed number of event iterations (test helper).
pub fn run_frames<F>(tui: &mut Tui, mut f: F, frames: usize) -> io::Result<()>
where
    F: FnMut(&mut Tui) -> bool,
{
    for _ in 0..frames {
        if !f(tui) {
            break;
        }
        let _ = tui.poll_event(Some(Duration::from_millis(5)));
    }
    Ok(())
}

