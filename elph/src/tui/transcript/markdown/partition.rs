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

/// Advance through fully closed fenced blocks that start at or after `boundary`.
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
        let block_end = raw[after_close..search_end]
            .find('\n')
            .map(|index| after_close + index + 1)
            .unwrap_or(search_end);
        if block_end > end {
            end = block_end;
        }
        scan = block_end.max(after_close);
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
        let fence_end = raw.find("```\nnext").map(|index| index + 4).expect("close fence");
        assert!(
            find_stable_boundary(raw, false) >= fence_end,
            "expected stable prefix through closed fence"
        );
    }
}
