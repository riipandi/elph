//! Word-wrap row counting that mirrors iocraft's `TextWrap::Wrap` paint path.
//!
//! iocraft wraps `Text`/`MixedText` content with its internal `SegmentedString::wrap`, which
//! breaks at Unicode line-break opportunities (UAX #14, via a vendored `unicode-linebreak` with
//! Unicode 15.0.0 tables) and greedy-fills each line. Transcript measurement historically
//! counted rows with a *character-wrap* layout ([`crate::text_input_layout::WrappedTextLayout`])
//! that packs more characters per row. At narrow widths the two diverge: measured rows end up
//! smaller than painted rows, so the scroll viewport pins its bottom above the painted tail and
//! the last rows (often the `…` of a truncated line) get clipped.
//!
//! [`wrapped_text_row_count`] replicates the single-segment case of iocraft's wrap algorithm —
//! row count only, no segment mapping — using the same `unicode-linebreak` tables so measurement
//! matches paint exactly. Text that is pre-wrapped and rendered with `TextWrap::NoWrap` (tables,
//! code blocks, sticky chrome) stays on the character-wrap path and must not use this function.

use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_width::UnicodeWidthChar;

/// Rows iocraft paints when `text` is wrapped at `wrap_width` with `TextWrap::Wrap`.
///
/// Mirrors `SegmentedString::wrap` for a single segment: mandatory breaks at `\n`, greedy
/// breaks at Unicode line-break opportunities, forced mid-word breaks for over-wide runs, and
/// one extra (empty) row when the text ends with a newline.
pub fn wrapped_text_row_count(text: &str, wrap_width: usize) -> usize {
    if text.is_empty() {
        return 0;
    }

    let mut lines = 0usize;
    let mut current_width = 0usize;
    let mut line_start = 0usize;

    for (break_pos, opportunity) in linebreaks(text) {
        // Segment covering [line_start, break_pos): everything since the previous break point,
        // including the `\n` of a mandatory break (zero width, so it does not affect row math).
        let segment = &text[line_start..break_pos];
        let segment_width = str_display_width(segment);
        let trailing_whitespace_width = trailing_whitespace_width(segment);
        line_start = break_pos;

        let fit_width = segment_width.saturating_sub(trailing_whitespace_width);
        if current_width + fit_width <= wrap_width {
            current_width += segment_width;
        } else {
            if current_width > 0 {
                lines += 1;
            }
            // The segment itself is too wide: force-break it, skipping trailing whitespace.
            // `current_width` is rebuilt below (either from the wrapped remainder or the whole
            // segment), mirroring iocraft replacing its current line.
            let content_end = segment
                .char_indices()
                .rev()
                .take_while(|(_, c)| c.is_whitespace())
                .last()
                .map(|(i, _)| i)
                .unwrap_or(segment.len());
            let content_width = str_display_width(&segment[..content_end]);
            if content_width > wrap_width {
                let mut w = 0usize;
                let mut start = 0usize;
                for (idx, c) in segment[..content_end].char_indices() {
                    let char_width = char_display_width(c);
                    if w > 0 && w + char_width > wrap_width {
                        lines += 1;
                        w = 0;
                        start = idx;
                    }
                    w += char_width;
                }
                current_width = str_display_width(&segment[start..]);
            } else {
                current_width = segment_width;
            }
        }

        if opportunity == BreakOpportunity::Mandatory {
            lines += 1;
            current_width = 0;
        }
    }

    // iocraft appends one extra (empty) line when the last segment ends with a newline.
    if text.ends_with('\n') {
        lines += 1;
    }

    lines
}

/// Display width of a char as iocraft computes it (unicode-width 0.1.x): control characters
/// (including `\n`, `\r`, `\t`) have zero width. Newer unicode-width releases report them as 1,
/// which would skew the fit checks against iocraft's paint.
fn char_display_width(c: char) -> usize {
    if c.is_control() { 0 } else { c.width().unwrap_or(0) }
}

fn str_display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

fn trailing_whitespace_width(s: &str) -> usize {
    s.chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .map(char_display_width)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iocraft_test_vectors_match_segmented_string_wrap() {
        // Ported from iocraft-0.8.4 `SegmentedString::wrap` tests (single-segment cases):
        // "Hello, world! This is a test string." @ 12 → ["Hello, ", "world! This ", "is a test ", "string."]
        assert_eq!(wrapped_text_row_count("Hello, world! This is a test string.", 12), 4);
        // @ 0 → ["f", "o", "o ", "b", "a", "r"]
        assert_eq!(wrapped_text_row_count("foo bar", 0), 6);
        // @ 11 → ["Hello, ", "world! This ", "is a test ", "string."]
        assert_eq!(wrapped_text_row_count("Hello, world! This is a test string.", 11), 4);
        // "Hello, thisisalongunbreakablemultiline str." @ 12 → 4 rows with forced mid-word breaks
        assert_eq!(wrapped_text_row_count("Hello, thisisalongunbreakablemultiline str.", 12), 4);
        // "Hello, this\nstring\nhas\nnewlines in it.\n\n" @ 11 → ["Hello, this", "string", "has", "newlines in ", "it.", "", ""]
        assert_eq!(wrapped_text_row_count("Hello, this\nstring\nhas\nnewlines in it.\n\n", 11), 7);
        // "this is a wrapping test" @ 14 → ["this is a ", "wrapping test"]
        assert_eq!(wrapped_text_row_count("this is a wrapping test", 14), 2);
    }

    #[test]
    fn empty_and_edge_cases() {
        assert_eq!(wrapped_text_row_count("", 10), 0);
        assert_eq!(wrapped_text_row_count("a", 1), 1);
        assert_eq!(wrapped_text_row_count("abc", 1), 3);
        assert_eq!(wrapped_text_row_count("abc", 3), 1);
        // "ab cd" @ 1 → ["a", "b ", "c", "d"]
        assert_eq!(wrapped_text_row_count("ab cd", 1), 4);
        // single char wider than wrap width still occupies one row
        assert_eq!(wrapped_text_row_count("界", 0), 1);
    }

    #[test]
    fn newlines_are_mandatory_breaks() {
        assert_eq!(wrapped_text_row_count("a\nb", 5), 2);
        // trailing newline adds one (empty) row, mirroring SegmentedString::wrap
        assert_eq!(wrapped_text_row_count("a\n", 5), 2);
        assert_eq!(wrapped_text_row_count("a\n\n", 5), 3);
        assert_eq!(wrapped_text_row_count("a\nb\n", 5), 3);
        // long line still wraps inside each \n-separated segment
        assert_eq!(wrapped_text_row_count("aaaa bbbb\ncccc dddd", 5), 4);
    }

    #[test]
    fn double_width_chars_use_unicode_width() {
        // 5 double-width kana = 10 columns; wrap(6) → ["こんに", "ちは"]
        assert_eq!(wrapped_text_row_count("こんにちは", 6), 2);
        // wrap(4) → ["こん", "にち", "は"]
        assert_eq!(wrapped_text_row_count("こんにちは", 4), 3);
        // ASCII + double-width mix: "ab こんにちは cd" @ 8 → ["ab ", "こんにち", "は cd"]
        assert_eq!(wrapped_text_row_count("ab こんにちは cd", 8), 3);
    }
}
