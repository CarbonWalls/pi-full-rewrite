//! pi-tui headless tests: diff renderer, markdown, editor.

use pi_tui::buffer::{diff, render_to_string, Buffer, Color};
use pi_tui::editor::Editor;
use pi_tui::markdown;
use pi_tui::Widget;

#[test]
fn diff_skips_unchanged_cells() {
    let mut prev = Buffer::new(10, 5);
    prev.print(0, 0, "hello", &Color::default());
    let mut cur = Buffer::new(10, 5);
    cur.print(0, 0, "hello", &Color::default());
    let mut out = String::new();
    diff(&prev, &cur, &mut out);
    assert!(
        !out.contains("\x1b[38") && !out.contains("h"),
        "unchanged buffer should emit no glyph updates: {out}"
    );
}

#[test]
fn diff_updates_changed_cell_with_position_and_glyph() {
    let mut prev = Buffer::new(10, 5);
    prev.print(0, 0, "abcde", &Color::default());
    let mut cur = Buffer::new(10, 5);
    cur.print(0, 0, "abXde", &Color::default());
    let mut out = String::new();
    diff(&prev, &cur, &mut out);
    assert!(out.contains("\x1b[1;3H"), "should target row1 col3");
    assert!(out.contains('X'));
}

#[test]
fn diff_applies_style_changes() {
    let prev = Buffer::new(10, 3);
    let mut cur = Buffer::new(10, 3);
    let mut style = Color::default().fg_rgb(255, 0, 0);
    style.bold = true;
    cur.print(0, 0, "hi", &style);
    let mut out = String::new();
    diff(&prev, &cur, &mut out);
    assert!(out.contains("38;2;255;0;0"), "truecolor fg expected");
    assert!(out.contains("\x1b[1m"), "bold expected");
}

#[test]
fn diff_size_change_triggers_clear() {
    let prev = Buffer::new(10, 5);
    let cur = Buffer::new(20, 10);
    let mut out = String::new();
    diff(&prev, &cur, &mut out);
    assert!(out.contains("\x1b[2J"));
}

#[test]
fn buffer_print_clips_and_overwrites() {
    let mut b = Buffer::new(4, 1);
    b.print(2, 0, "abcdef", &Color::default()); // writes x=2,3 then clips
    let s = render_to_string(&b);
    assert_eq!(s, "  ab");
}

#[test]
fn markdown_renders_headings_and_inline() {
    let md = "# Title\n\nSome **bold** and `code` and *em*.\n- item one\n- item two";
    let lines = markdown::render(md);
    assert_eq!(lines.len(), 5);
    // heading run is bold
    let h = &lines[0].runs;
    assert!(h[0].style.bold);
    assert_eq!(h[0].text, "Title");
    // inline line (index 2) has bold + code runs
    let inline = &lines[2];
    let joined = inline.text();
    assert!(joined.contains("bold") && joined.contains("code"));
    assert!(inline.runs.iter().any(|r| r.style.bold)); // **bold**
    assert!(inline.runs.iter().any(|r| r.text == "code")); // `code`
    // bullet lines keep the marker
    assert!(lines[4].text().contains("item two"));
}

#[test]
fn editor_insert_delete_cursor() {
    use crossterm::event::KeyCode;
    let mut e = Editor::new();
    assert!(e.handle_key(KeyCode::Char('a')));
    assert!(e.handle_key(KeyCode::Char('b')));
    assert_eq!(e.text(), "ab");
    assert_eq!(e.cursor, (0, 2));
    assert!(e.handle_key(KeyCode::Left));
    assert_eq!(e.cursor, (0, 1));
    assert!(e.handle_key(KeyCode::Backspace)); // removes 'a' -> "b", cursor col 0
    assert_eq!(e.text(), "b");
    assert!(e.handle_key(KeyCode::Char('c'))); // inserts at col 0 -> "cb"
    assert_eq!(e.text(), "cb");
}

#[test]
fn editor_newline_and_commit() {
    use crossterm::event::KeyCode;
    let mut e = Editor::new();
    e.handle_key(KeyCode::Char('a'));
    e.newline(); // hard newline
    e.handle_key(KeyCode::Char('b'));
    assert_eq!(e.text(), "a\nb");
    let committed = e.commit();
    assert!(committed.is_some());
}

#[test]
fn editor_placeholder_render() {
    let mut e = Editor::new();
    e.placeholder = "ask me...".into();
    let mut b = Buffer::new(40, 5);
    let area = pi_tui::widget::Area::new(0, 0, 40, 5);
    e.render(&mut b, &area);
    assert!(b.cursor.is_some(), "editor should show a cursor");
    let s = render_to_string(&b);
    assert!(s.contains("ask me..."));
}

#[test]
fn scroll_render_shows_history() {
    use pi_tui::Area;
    let mut md = String::new();
    for i in 0..30 {
        md.push_str(&format!("line {i}\n"));
    }
    let theme = pi_tui::Theme::default();
    let area = Area::new(0, 0, 40, 5);
    // tail anchored: newest five lines
    let mut buf = Buffer::new(40, 5);
    pi_tui::draw_markdown_scroll_with(&mut buf, &area, &md, 5, &theme, 0);
    let txt = render_to_string(&buf);
    assert!(txt.contains("line 25"), "tail anchored: {txt}");
    assert!(!txt.contains("line 0"), "history hidden: {txt}");
    // scroll_back far enough: oldest five lines
    let mut buf2 = Buffer::new(40, 5);
    pi_tui::draw_markdown_scroll_with(&mut buf2, &area, &md, 5, &theme, 30);
    let txt2 = render_to_string(&buf2);
    assert!(txt2.contains("line 0") && txt2.contains("line 4"), "history: {txt2}");
}

#[test]
fn spinner_width_accounts_for_frame_and_label() {
    let mut s = pi_tui::Spinner::new("working");
    s.spin = true;
    // some frames are empty (breathing dots); width never drops below label
    assert!(s.width() >= "working".chars().count(), "width: {}", s.width());
    s.spin = false;
    // render() shows "• working" (frame + space + label)
    assert_eq!(s.width(), "• working".chars().count());
}
