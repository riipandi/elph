use elph_tui::components::scroll_bar::{ScrollbarStyle, scrollbar_thumb_rows};
use elph_tui::components::scroll_box::scroll_view_max_offset;

#[test]
fn max_offset_when_content_fits_viewport() {
    assert_eq!(scroll_view_max_offset(10, 20), 0);
}

#[test]
fn max_offset_when_content_overflows() {
    assert_eq!(scroll_view_max_offset(50, 20), 30);
}

#[test]
fn dark_style_has_colors() {
    let style = ScrollbarStyle::dark();
    assert!(style.thumb_color.is_some());
    assert!(style.track_color.is_some());
}

#[test]
fn thumb_rows_fill_viewport_when_content_fits() {
    assert_eq!(scrollbar_thumb_rows(10, 8), 10);
    assert_eq!(scrollbar_thumb_rows(10, 10), 10);
}

#[test]
fn thumb_rows_scales_with_overflow() {
    assert_eq!(scrollbar_thumb_rows(10, 100), 1);
    assert_eq!(scrollbar_thumb_rows(20, 100), 4);
}
