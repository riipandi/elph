use super::markdown::{MarkdownTheme, render_markdown_lines};
use super::markdown_table::{is_gfm_table_row, render_gfm_pipe_table};

/// Partition streaming markdown into a stable block (safe for full parsing) and a live tail.
pub fn partition_streaming_markdown(text: &str, streaming: bool) -> (&str, &str) {
    if !streaming {
        return (text, "");
    }
    if text.is_empty() {
        return ("", "");
    }
    if text.ends_with('\n') {
        return (text, "");
    }
    match text.rfind('\n') {
        Some(pos) => (&text[..=pos], &text[pos + 1..]),
        None => ("", text),
    }
}

/// Render assistant markdown during streaming, including GFM tables in the stable prefix.
pub fn render_streaming_markdown_lines(
    text: &str,
    width: u16,
    theme: MarkdownTheme,
    streaming: bool,
    show_cursor: bool,
) -> Vec<String> {
    let (stable, tail) = partition_streaming_markdown(text, streaming);
    let mut lines = Vec::new();

    if !stable.is_empty() {
        lines.extend(render_markdown_lines(stable, width, theme));
    }

    if tail.is_empty() {
        if streaming && show_cursor && text.is_empty() {
            lines.push(cursor_line("", true, &theme));
        }
        return lines;
    }

    if is_gfm_table_row(tail) {
        let table_lines = vec![tail.to_string()];
        lines.extend(render_gfm_pipe_table(&table_lines, &theme));
        if streaming
            && show_cursor
            && let Some(last) = lines.last_mut()
        {
            last.push('▌');
        }
        return lines;
    }

    let tail_lines = render_markdown_lines(tail, width, theme);
    if tail_lines.is_empty() {
        lines.push(cursor_line(tail, show_cursor && streaming, &theme));
    } else if streaming && show_cursor {
        let mut tail_lines = tail_lines;
        if let Some(last) = tail_lines.last_mut() {
            last.push('▌');
        }
        lines.extend(tail_lines);
    } else {
        lines.extend(tail_lines);
    }

    lines
}

fn cursor_line(text: &str, show: bool, theme: &MarkdownTheme) -> String {
    let mut line = if text.is_empty() {
        String::new()
    } else {
        theme.paint_text(text)
    };
    if show {
        line.push('▌');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_partial_last_line() {
        let (stable, tail) = partition_streaming_markdown("Hello\nwor", true);
        assert_eq!(stable, "Hello\n");
        assert_eq!(tail, "wor");
    }

    #[test]
    fn stable_prefix_renders_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n";
        let lines = render_streaming_markdown_lines(md, 40, MarkdownTheme::dark(), true, false);
        let joined = lines.join("\n");
        assert!(joined.contains('┌'));
        assert!(!joined.contains("| 1 | 2 |"));
        assert!(joined.contains('1'));
    }

    #[test]
    fn tail_table_row_renders_while_streaming() {
        let md = "| Name | Age |\n|------|-----|\n| Ali";
        let lines = render_streaming_markdown_lines(md, 40, MarkdownTheme::dark(), true, false);
        let joined = lines.join("\n");
        assert!(joined.contains("Name"));
        assert!(joined.contains("Ali"));
    }
}
