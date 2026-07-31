//! Regression test: `/tools list` markdown measurement must match iocraft's painted rows.
//!
//! The transcript auto-scroll pins its viewport bottom to the *measured* height. When
//! measurement under-counted vs the word-wrapped paint (char-wrap vs word-wrap divergence at
//! narrow widths), the painted tail (`…`) fell outside the viewport and the card appeared
//! clipped mid-line. The `find_path` description is long enough to wrap at every terminal
//! width, making this a sensitive probe for the parity.

use elph::agent::tools_slash_message;
use elph_tui::components::markdown::{markdown_document_row_count, parse_markdown_document, render_markdown_block};
use iocraft::prelude::*;

#[test]
fn tools_list_measure_matches_paint_at_all_widths() {
    let message = tools_slash_message(None, "list").expect("builtin tools list");
    let doc = parse_markdown_document(&message);

    for width in [
        36u16, 38, 40, 42, 44, 46, 48, 50, 60, 70, 73, 77, 80, 93, 97, 100, 113, 120,
    ] {
        let measured = markdown_document_row_count(&doc, width);
        let block = render_markdown_block(&doc, width);
        let rendered = element! { View(width: width) { #(vec![block]) } }.to_string();
        let rendered_rows = rendered.lines().count();
        assert_eq!(
            measured as usize, rendered_rows,
            "width {width}: measured {measured} != painted {rendered_rows}"
        );
    }
}
