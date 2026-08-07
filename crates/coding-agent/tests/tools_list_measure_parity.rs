//! Regression test: `/tools` plain-text output measurement must match iocraft's painted rows.
//!
//! `/tools` renders in a scrollable dialog (plain text, `TextWrap::Wrap`). The viewport pins to
//! the measured height, so the word-wrapped row count must match the paint at every width. The
//! `find_path` description is long enough to wrap at every terminal width, making this a
//! sensitive probe for the parity.

use elph::agent::tools_slash_message;
use elph_tui::wrapped_text_row_count;
use iocraft::prelude::*;

#[test]
fn tools_plain_text_measure_matches_paint_at_all_widths() {
    let message = tools_slash_message(None).expect("builtin tools list");
    // Plain-text layout — no markdown table or bullet syntax.
    assert!(!message.contains("| Tool |"));
    assert!(!message.contains("- **`"));

    for width in [
        36u16, 38, 40, 42, 44, 46, 48, 50, 60, 70, 73, 77, 80, 93, 97, 100, 113, 120,
    ] {
        let measured = wrapped_text_row_count(&message, width as usize);
        let rendered =
            element! { View(width: width) { Text(content: message.clone(), wrap: TextWrap::Wrap) } }.to_string();
        let rendered_rows = rendered.lines().count();
        assert_eq!(
            measured as usize, rendered_rows,
            "width {width}: measured {measured} != painted {rendered_rows}"
        );
    }
}
