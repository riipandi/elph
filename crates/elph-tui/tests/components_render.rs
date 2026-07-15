use elph_tui::components::ascii_font::render_bitmap;
use elph_tui::components::code::highlight_rust_line;
use elph_tui::components::diff::unified_lines;
use elph_tui::components::frame_buffer::FrameBuffer;
use elph_tui::components::qr_code::render_qr;

#[test]
fn highlights_keywords() {
    let parts = highlight_rust_line("fn main() {}");
    assert!(!parts.is_empty());
}

#[test]
fn unified_diff_non_empty() {
    let lines = unified_lines("a\n", "b\n");
    assert!(!lines.is_empty());
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
