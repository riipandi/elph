/// Zero-width APC marker emitted before the fake cursor for IME positioning.
pub const CURSOR_MARKER: &str = "\x1b_pi:c\x07";

/// Per-line reset appended by the diff renderer (SGR + OSC 8 hyperlink reset).
pub const LINE_RESET: &str = "\x1b[0m\x1b]8;;\x07";

/// Hardware cursor position extracted from rendered lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub line: usize,
    pub col: usize,
}

/// Finds [`CURSOR_MARKER`] in `lines`, strips it, and returns the cursor position.
pub fn extract_and_strip_cursor(lines: &mut [String]) -> Option<CursorPosition> {
    use crate::utils::str_display_width;

    for (line_idx, line) in lines.iter_mut().enumerate() {
        if let Some(marker_pos) = line.find(CURSOR_MARKER) {
            let before = &line[..marker_pos];
            let after = &line[marker_pos + CURSOR_MARKER.len()..];
            let col = str_display_width(before);
            *line = format!("{before}{after}");
            return Some(CursorPosition { line: line_idx, col });
        }
    }
    None
}
