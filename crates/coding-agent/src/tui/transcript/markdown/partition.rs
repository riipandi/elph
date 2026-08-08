//! Conservative stable-boundary detection for streaming markdown.
//!
//! Only top-level (depth=0) closed blocks are safe to freeze; content inside
//! open lists or blockquotes stays in the streaming tail.

/// Byte index through which `raw` is safe to parse as markdown.
///
/// When `force_flush` is true (turn complete), the entire buffer is stable.
pub fn find_stable_boundary(raw: &str, force_flush: bool) -> usize {
    if raw.is_empty() {
        return 0;
    }
    if force_flush {
        return raw.len();
    }

    let search_end = fence_safe_end(raw);
    let slice = &raw[..search_end];
    let mut boundary = if let Some(pos) = slice.rfind("\n\n") {
        pos + 2
    } else {
        0
    };
    boundary = extend_past_closed_fences(raw, boundary, search_end);
    boundary = extend_past_closed_tables(raw, boundary, search_end);

    while boundary > 0
        && (has_unclosed_inline_markers(&raw[..boundary]) || elph_tui::markdown_has_open_container_at(raw, boundary))
    {
        match raw[..boundary.saturating_sub(2)].rfind("\n\n") {
            Some(pos) => boundary = pos + 2,
            None => {
                boundary = 0;
                break;
            }
        }
    }

    boundary
}

/// Advance through complete GFM tables that live at or after `boundary`.
///
/// GFM tables are terminated only by a blank line, a block boundary, or end-of-input — there
/// is no closing fence. Without this, a table at the end of a message (no trailing blank line)
/// never becomes stable: it stays in the streaming tail forever, where a long reply's
/// 4K cap can truncate it or its neighbors.
///
/// A table is "complete" when it has a header separator row (`| --- | --- |`), so we never
/// freeze a half-typed table.
///
/// The scanner first skips prose lines at the START of the current paragraph segment: GFM
/// tables are often glued straight onto a preceding sentence (`"as seen below:\n| A | B |\n| --- | --- |"`),
/// which pulldown parses as `Paragraph` + `Table`. Freezing only tables that begin exactly at
/// `boundary` left those glued tables in the streaming tail forever → raw `|` rows on screen.
fn extend_past_closed_tables(raw: &str, boundary: usize, search_end: usize) -> usize {
    let mut end = boundary;
    let mut scan = boundary;

    // Skip non-table prose at the head of the segment (same paragraph as the table).
    while scan < search_end {
        let line_end = raw[scan..search_end]
            .find('\n')
            .map(|nl| scan + nl + 1)
            .unwrap_or(search_end);
        let trimmed = raw[scan..line_end].trim();
        if trimmed.is_empty() {
            // Blank line ends the segment before any table — nothing glued here.
            return end;
        }
        if is_table_line(trimmed) {
            break;
        }
        scan = line_end;
    }

    // Scan the table run: header → separator → data rows until a blank / non-table line.
    let mut seen_separator = false;
    let mut table_end = 0usize;

    while scan <= search_end {
        let line_end = raw[scan..search_end]
            .find('\n')
            .map(|nl| scan + nl + 1)
            .unwrap_or(search_end);
        let line = &raw[scan..line_end];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Blank line ends the table (but does not extend the boundary past the blank).
            if seen_separator && table_end > end {
                end = table_end;
            }
            break;
        }

        if is_table_line(trimmed) {
            table_end = line_end;
            if is_table_separator_row(trimmed) {
                seen_separator = true;
            }
        } else {
            // Non-table line — the table (if any) ended before this line.
            if seen_separator && table_end > end {
                end = table_end;
            }
            break;
        }
        scan = line_end;
    }

    // Table runs to end of input (no blank line): freeze it if complete.
    if seen_separator && table_end > end {
        end = table_end;
    }
    end
}

/// A GFM table data row starts with `|` (or is all `:`/`|`/`-` for separators).
fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') || trimmed.starts_with('|')
}

/// A GFM header separator row: cells of `-`, `:`, and `|` (e.g. `| --- | :---: |`).
fn is_table_separator_row(line: &str) -> bool {
    let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
    if trimmed.is_empty() {
        return false;
    }
    let cells_text = trimmed.replace('|', " ");
    let cells_valid = cells_text.split_whitespace().all(|cell| {
        !cell.is_empty()
            && cell
                .trim_matches(':')
                .chars()
                .all(|c| c == '-' || c == ' ' || c == ':' || c == '|')
    });
    cells_valid
        && cells_text
            .split_whitespace()
            .all(|cell| cell.trim_matches(':').contains('-'))
}

/// Advance through fully closed fenced blocks that start at or after `boundary`.
/// This ensures complete codeblocks are frozen as stable, preventing truncation.
fn extend_past_closed_fences(raw: &str, boundary: usize, search_end: usize) -> usize {
    let mut end = boundary;
    let mut scan = boundary;
    while scan < search_end {
        let Some(rel_open) = raw[scan..search_end].find("```") else {
            break;
        };
        let open = scan + rel_open;
        let after_open = open + 3;
        let Some(rel_close) = raw[after_open..search_end].find("```") else {
            break;
        };
        let close = after_open + rel_close;
        let after_close = close + 3;

        // Include the entire codeblock including the closing fence
        let block_end = after_close;

        if block_end > end {
            end = block_end;
        }
        scan = block_end;
    }
    end
}

/// Cap parsing before an unclosed fenced code block.
fn fence_safe_end(raw: &str) -> usize {
    let mut count = 0usize;
    let mut last_open = 0usize;
    let mut pos = 0usize;
    while let Some(rel) = raw[pos..].find("```") {
        let abs = pos + rel;
        count += 1;
        last_open = abs;
        pos = abs + 3;
    }
    pos = 0;
    while let Some(rel) = raw[pos..].find("~~~") {
        let abs = pos + rel;
        count += 1;
        last_open = last_open.max(abs);
        pos = abs + 3;
    }
    if count % 2 == 1 { last_open } else { raw.len() }
}

fn has_unclosed_inline_markers(slice: &str) -> bool {
    if slice.is_empty() {
        return false;
    }
    let tail = slice.rsplit_once('\n').map(|(_, line)| line).unwrap_or(slice);
    odd_count(tail, "**") || odd_backtick_count(tail) || has_unclosed_bracket(tail) || has_unclosed_html_tag(tail)
}

fn odd_count(haystack: &str, needle: &str) -> bool {
    let mut count = 0usize;
    let mut pos = 0usize;
    while let Some(rel) = haystack[pos..].find(needle) {
        count += 1;
        pos += rel + needle.len();
    }
    count % 2 == 1
}

fn odd_backtick_count(line: &str) -> bool {
    let mut count = 0usize;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            let mut run = 1usize;
            while chars.peek() == Some(&'`') {
                chars.next();
                run += 1;
            }
            if run == 1 {
                count += 1;
            }
        }
    }
    count % 2 == 1
}

fn has_unclosed_bracket(line: &str) -> bool {
    let mut open = 0i32;
    let mut in_link_dest = false;
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '[' if !in_link_dest => open += 1,
            ']' => {
                if open > 0 {
                    open -= 1;
                }
                if index + 1 < chars.len() && chars[index + 1] == '(' {
                    in_link_dest = true;
                }
            }
            ')' if in_link_dest => in_link_dest = false,
            _ => {}
        }
        index += 1;
    }
    open > 0 || in_link_dest
}

fn has_unclosed_html_tag(line: &str) -> bool {
    let Some(start) = line.rfind('<') else {
        return false;
    };
    let tail = &line[start..];
    if tail.starts_with("</") {
        return !tail.contains('>');
    }
    if tail.starts_with("<!--") {
        return !tail.contains("-->");
    }
    !tail.contains('>')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_at_end_of_stream_stays_in_stable_prefix() {
        // A GFM table at the end (no following \n\n) must be fully included in the stable
        // prefix once it's syntactically complete — otherwise it renders as broken text
        // during streaming.
        let raw = "## Tools\n\n| Tool | Status |\n| --- | --- |\n| grep | ✅ |\n";
        let boundary = find_stable_boundary(raw, false);
        assert_eq!(
            boundary,
            raw.len(),
            "complete table at end must be stable, got boundary {boundary} < {}",
            raw.len()
        );
    }

    #[test]
    fn table_glued_to_previous_paragraph_stays_in_stable_prefix() {
        // Models often emit a closing sentence and IMMEDIATELY a table without a blank
        // line in between (`"as seen below:\n| A | B |\n| --- | --- |\n| 1 | 2 |"`).
        // pulldown parses that as paragraph + table, so the stable boundary must extend
        // through the whole complete table — otherwise it lingers in the streaming tail
        // where the 4K cap can shred it into raw-markdown-looking fragments.
        for raw in [
            "As seen below:\n| Feature | Status |\n| --- | --- |\n| a | ✅ |",
            "Sizes:\n| Name | Size |\n| :--- | ---: |\n| x | 1 |\n| y | 2 |\n",
        ] {
            let boundary = find_stable_boundary(raw, false);
            assert_eq!(
                boundary,
                raw.len(),
                "complete table glued to previous prose must be stable, got boundary {boundary} < {}",
                raw.len()
            );
        }
    }

    #[test]
    fn incomplete_table_stays_in_tail() {
        // While the table is still missing its header separator row (only the header line is
        // present), the boundary stops before it so no half-table is frozen.
        let raw = "## Tools\n\n| Tool | Status |\n";
        let boundary = find_stable_boundary(raw, false);
        assert!(
            boundary < raw.len(),
            "table without separator must stay in tail (got boundary {boundary})"
        );
        // The boundary should stop right after the heading paragraph (before the table).
        let heading_block = raw.find("\n\n").map(|i| i + 2).unwrap_or(0);
        assert_eq!(boundary, heading_block, "boundary stops after heading, before table");
    }

    #[test]
    fn half_typed_table_glued_to_prose_stays_in_tail() {
        // Even when glued to prose, a table WITHOUT the separator row (still being typed)
        // must NOT be frozen — freezing a half table would render a broken grid.
        let raw = "Summary:\n| Tool | Status |\n";
        let boundary = find_stable_boundary(raw, false);
        assert_eq!(
            boundary, 0,
            "header-only glued table must not freeze (no separator row), got {boundary}"
        );
    }

    #[test]
    fn complete_table_with_data_rows_is_stable() {
        let raw = "## Tools\n\n| Tool | Status |\n| --- | --- |\n| grep | ✅ |\n| rg | ✅ |\n";
        let boundary = find_stable_boundary(raw, false);
        assert_eq!(
            boundary,
            raw.len(),
            "complete multi-row table at end must be stable, got {boundary}"
        );
    }

    #[test]
    fn empty_buffer_has_no_stable_prefix() {
        assert_eq!(find_stable_boundary("", false), 0);
    }

    #[test]
    fn paragraph_boundary_stabilizes_prefix() {
        let raw = "# Title\n\nBody one.\nPartial";
        assert_eq!(find_stable_boundary(raw, false), 9);
    }

    #[test]
    fn unclosed_fence_defers_stability() {
        let raw = "intro\n\n```rust\nlet x = 1;\nstill typing";
        assert_eq!(find_stable_boundary(raw, false), 7);
    }

    #[test]
    fn closed_fence_allows_stability_after_block() {
        let raw = "intro\n\n```rust\nlet x = 1;\n```\n\nDone.";
        let stable_through_fence = raw.find("Done.").expect("tail");
        assert_eq!(find_stable_boundary(raw, false), stable_through_fence);
        assert_eq!(find_stable_boundary(raw, true), raw.len());
    }

    #[test]
    fn force_flush_returns_full_length() {
        let raw = "```open\npartial";
        assert_eq!(find_stable_boundary(raw, true), raw.len());
    }

    #[test]
    fn unclosed_bold_defers_stability_past_paragraph() {
        let raw = "intro\n\n**still typing";
        assert_eq!(find_stable_boundary(raw, false), 7);
    }

    #[test]
    fn closed_bold_allows_stability() {
        let raw = "intro\n\n**done**\n\n";
        assert_eq!(find_stable_boundary(raw, false), raw.len());
    }

    #[test]
    fn closed_fence_stabilizes_without_trailing_paragraph_break() {
        let raw = "intro\n\n```rust\nlet x = 1;\n```\nnext line";
        let fence_close_pos = raw.find("```").expect("fence open");
        let fence_open = fence_close_pos + 3; // After ```
        let fence_close = raw[fence_open..]
            .find("```")
            .map(|i| fence_open + i)
            .expect("fence close");
        let fence_end = fence_close + 3; // After ```
        let stable_boundary = find_stable_boundary(raw, false);
        assert!(
            stable_boundary >= fence_end,
            "expected stable prefix through closed fence, got {} < {} (fence_end={})",
            stable_boundary,
            fence_end,
            fence_end
        );
    }
}
