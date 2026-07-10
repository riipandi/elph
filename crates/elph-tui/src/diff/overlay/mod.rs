mod composite;
mod layout;

pub use composite::composite_line_at;
pub(crate) use composite::composite_overlays;
pub(crate) use layout::OverlayEntry;
pub use layout::{
    OverlayAnchor, OverlayHandle, OverlayLayout, OverlayMargin, OverlayOptions, SizeValue, resolve_layout,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_overlay_horizontally() {
        let options = OverlayOptions {
            width: Some(SizeValue::Absolute(40)),
            ..Default::default()
        };
        let layout = resolve_layout(&options, 3, 80, 24).unwrap();
        assert_eq!(layout.col, 20);
        assert_eq!(layout.width, 40);
    }

    #[test]
    fn composites_overlay_line() {
        use crate::utils::str_display_width;

        let base = "hello world";
        let out = composite_line_at(base, "XX", 6, 2, 11);
        assert!(out.contains("XX"));
        assert!(str_display_width(&out) >= 8);
    }
}
