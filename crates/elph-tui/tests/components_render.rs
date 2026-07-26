use elph_tui::components::UiTheme;
use elph_tui::components::ascii_font::render_bitmap;
use elph_tui::components::code::highlight_rust_line;
use elph_tui::components::diff::compute::compute_diff;
use elph_tui::components::diff::render::{DiffLineNumberStyle, render_unified_hunk};
use elph_tui::components::frame_buffer::FrameBuffer;
use elph_tui::components::qr_code::render_qr;

#[test]
fn highlights_keywords() {
    let parts = highlight_rust_line("fn main() {}", UiTheme::default());
    assert!(!parts.is_empty());
}

#[test]
fn unified_diff_non_empty() {
    let result = compute_diff("a\n", "b\n", 3);
    let theme = UiTheme::default();
    let elements = render_unified_hunk(&result.hunks[0], None, false, DiffLineNumberStyle::None, theme, 40);
    assert!(!elements.is_empty());
}

#[test]
fn renders_qr() {
    let grid = render_qr("elph", "█", " ");
    assert!(grid.contains('█'));
}

#[test]
fn bitmap_non_empty() {
    let out = render_bitmap("ELPH");
    assert!(out.contains('█'));
}

#[test]
fn writes_cells() {
    let mut buf = FrameBuffer::new(5, 2);
    buf.set_text(1, 0, "hi");
    assert_eq!(buf.line(0), " hi  ");
}
