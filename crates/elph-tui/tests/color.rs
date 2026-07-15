use elph_tui::color::{from_hex, rgb};
use iocraft::prelude::Color;

#[test]
fn parses_hex() {
    assert_eq!(from_hex("#FF00AA"), Some(rgb(255, 0, 170)));
    assert_eq!(from_hex("3B82F6"), Some(rgb(59, 130, 246)));
}

#[test]
fn rejects_invalid_hex() {
    assert_eq!(from_hex("not-a-color"), None);
    assert_eq!(from_hex("#GGGGGG"), None);
    assert_eq!(from_hex("#12345"), None);
    assert_eq!(from_hex(""), None);
}

#[test]
fn rgb_builds_color() {
    assert_eq!(rgb(1, 2, 3), Color::Rgb { r: 1, g: 2, b: 3 });
}
