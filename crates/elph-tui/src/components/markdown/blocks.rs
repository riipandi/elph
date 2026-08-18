//! Re-export rendown block metrics used by iocraft paint / measure.

pub use rendown::{
    CODE_BLOCK_INSET_H, CODE_BLOCK_INSET_V, CODE_VERTICAL_PADDING, code_content_width, segment_end, segment_gap_after,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::parse_markdown_document;
    use rendown::BLOCK_GAP_ROWS;

    #[test]
    fn segment_gap_after_last_code_block_is_zero() {
        let doc = parse_markdown_document("```\ncode\n```");
        let end = segment_end(&doc.lines, 0);
        assert_eq!(segment_gap_after(&doc.lines, 0, end), 0);
    }

    #[test]
    fn segment_gap_after_code_before_paragraph_is_one() {
        let doc = parse_markdown_document("```\ncode\n```\n\nAfter");
        let code_end = segment_end(&doc.lines, 0);
        assert_eq!(segment_gap_after(&doc.lines, 0, code_end), BLOCK_GAP_ROWS);
    }

    #[test]
    fn code_content_width_reserves_horizontal_insets() {
        assert_eq!(code_content_width(20), 16);
        assert_eq!(code_content_width(3), 1);
    }

    #[test]
    fn adjacent_code_blocks_are_separate_segments_with_one_row_between() {
        let doc = parse_markdown_document("```\na\n```\n```\nb\n```");
        let first_end = segment_end(&doc.lines, 0);
        assert_eq!(first_end, 1, "first code block should not merge with the next");
        assert!(
            doc.lines.get(first_end).is_some_and(|line| line.is_blank()),
            "parser should insert a single blank row between adjacent fences"
        );
        assert_eq!(segment_gap_after(&doc.lines, 0, first_end), 0);
        let second_start = first_end + 1;
        let second_end = segment_end(&doc.lines, second_start);
        assert_eq!(second_end, doc.lines.len());
    }
}
