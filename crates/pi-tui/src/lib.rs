//! # pi-tui
//!
//! Terminal UI framework with cell-level differential rendering
//! (Rust rewrite of `@earendil-works/pi-tui`).

pub mod buffer;
pub mod editor;
pub mod markdown;
pub mod spinner;
pub mod theme;
pub mod tui;
pub mod widget;

pub use buffer::{Buffer, Cell, Color, diff, render_to_string};
pub use editor::Editor;
pub use markdown::{render, render_with, Run, StyledLine};
pub use spinner::Spinner;
pub use theme::Theme;
pub use tui::{Key, KeyEvent, Tui};
pub use widget::{Area, Column, TextBlock, Widget};

/// Draw a markdown string into a buffer at an area (used by the agent UI).
pub fn draw_markdown(buf: &mut Buffer, area: &Area, md: &str, max_lines: usize) {
    draw_markdown_with(buf, area, md, max_lines, &Theme::default());
}

/// Like [`draw_markdown_with`] but shifted `scroll_back` lines into the
/// history (`scroll_back = 0` is the tail-anchored default). Lets the TUI
/// implement PgUp/PgDn scrollback over long transcripts.
pub fn draw_markdown_scroll_with(
    buf: &mut Buffer,
    area: &Area,
    md: &str,
    max_lines: usize,
    theme: &Theme,
    scroll_back: usize,
) {
    if max_lines == 0 {
        return;
    }
    let lines = markdown::render_with(md, theme);
    let total = lines.len();
    // tail-anchored, then shift back by the scroll offset
    let tail_start = total.saturating_sub(max_lines);
    let start = tail_start.saturating_sub(scroll_back);
    for (i, line) in lines.iter().enumerate().skip(start) {
        let y = area.y + i - start;
        if y >= area.y + area.h || y >= buf.height {
            break;
        }
        let mut x = area.x;
        for run in &line.runs {
            if x >= area.x + area.w {
                break;
            }
            let text = run.text.clone();
            buf.print(x, y, &text, &run.style);
            x += text.chars().count();
        }
    }
}

/// Like [`draw_markdown`] but uses an explicit theme.
pub fn draw_markdown_with(
    buf: &mut Buffer,
    area: &Area,
    md: &str,
    max_lines: usize,
    theme: &Theme,
) {
    draw_markdown_scroll_with(buf, area, md, max_lines, theme, 0);
}
