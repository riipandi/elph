use unicode_width::UnicodeWidthStr;

use crate::utils::strip_ansi;

use super::ansi::{self, styled};
use super::markdown::MarkdownTheme;

/// Returns true for GFM table separator rows (`|---|---|`).
pub fn is_gfm_table_separator(line: &str) -> bool {
    let inner = line.trim().trim_matches('|').trim();
    !inner.is_empty() && inner.chars().all(|c| c == '-' || c == ':' || c == '|' || c == ' ')
}

/// Returns true when a trimmed line is a GFM pipe table row.
pub fn is_gfm_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.matches('|').count() >= 2
}

/// Parse `| cell | cell |` into trimmed cell strings.
pub fn parse_gfm_table_row(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
    trimmed.split('|').map(|c| c.trim().to_string()).collect()
}

struct TableRows {
    header: Option<Vec<String>>,
    data_rows: Vec<Vec<String>>,
}

fn split_gfm_table_lines(lines: &[String]) -> Option<TableRows> {
    if lines.is_empty() {
        return None;
    }

    let mut header: Option<Vec<String>> = None;
    let mut data_rows: Vec<Vec<String>> = Vec::new();
    let mut found_separator = false;

    for (i, line) in lines.iter().enumerate() {
        if is_gfm_table_separator(line) {
            found_separator = true;
            continue;
        }
        let row = parse_gfm_table_row(line);
        if i == 0 && !found_separator {
            header = Some(row);
        } else {
            data_rows.push(row);
        }
    }

    if !found_separator {
        if header.is_none() && !data_rows.is_empty() {
            header = Some(data_rows.remove(0));
        } else if header.is_some() && data_rows.is_empty() {
            // Header-only table while streaming.
        } else if header.is_none() && data_rows.is_empty() {
            return None;
        }
    }

    Some(TableRows { header, data_rows })
}

fn border_line(theme: &MarkdownTheme, left: char, mid: char, right: char, col_widths: &[usize]) -> String {
    let border = styled(&ansi::fg(theme.quote_border), "");
    let mut line = String::from(left);
    for (i, &w) in col_widths.iter().enumerate() {
        for _ in 0..w + 2 {
            line.push('─');
        }
        line.push(if i < col_widths.len() - 1 { mid } else { right });
    }
    format!("{border}{line}{}", ansi::RESET)
}

fn pad_cell(cell: &str, width: usize, prefix: &str) -> String {
    let plain = strip_ansi(cell);
    let cell_w = UnicodeWidthStr::width(plain.as_str());
    let padding = " ".repeat(width.saturating_sub(cell_w));
    format!("{prefix}{cell}{padding}")
}

/// Render header + body rows to ANSI box-drawing table lines.
pub fn render_gfm_table_data(
    header: Option<Vec<String>>,
    data_rows: Vec<Vec<String>>,
    theme: &MarkdownTheme,
) -> Vec<String> {
    let all_rows: Vec<&Vec<String>> = header.iter().chain(data_rows.iter()).collect();
    let col_count = all_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    let mut col_widths = vec![0usize; col_count];
    for row in &all_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                let plain = strip_ansi(cell);
                col_widths[i] = col_widths[i].max(UnicodeWidthStr::width(plain.as_str()));
            }
        }
    }

    let border = styled(&format!("{}{}", ansi::fg(theme.quote_border), ansi::DIM), "");
    let bold = format!("{}{}", ansi::fg(theme.text), ansi::BOLD);
    let mut out = Vec::new();

    out.push(border_line(theme, '┌', '┬', '┐', &col_widths));

    if let Some(hdr) = &header {
        let mut row = String::new();
        row.push_str(&border);
        row.push('│');
        for (i, w) in col_widths.iter().enumerate() {
            let raw = hdr.get(i).map(String::as_str).unwrap_or("");
            let painted = styled(&bold, raw);
            row.push(' ');
            row.push_str(&pad_cell(&painted, *w, ""));
            row.push_str(&format!(" {border}│"));
        }
        row.push_str(ansi::RESET);
        out.push(row);

        out.push(border_line(theme, '├', '┼', '┤', &col_widths));
    }

    for data in &data_rows {
        let mut row = String::new();
        row.push_str(&border);
        row.push('│');
        for (i, w) in col_widths.iter().enumerate() {
            let raw = data.get(i).map(String::as_str).unwrap_or("");
            let painted = theme.paint_text(raw);
            row.push(' ');
            row.push_str(&pad_cell(&painted, *w, ""));
            row.push_str(&format!(" {border}│"));
        }
        row.push_str(ansi::RESET);
        out.push(row);
    }

    out.push(border_line(theme, '└', '┴', '┘', &col_widths));
    out
}

/// Render a GFM pipe table block to ANSI lines with box-drawing borders.
pub fn render_gfm_pipe_table(lines: &[String], theme: &MarkdownTheme) -> Vec<String> {
    let Some(table) = split_gfm_table_lines(lines) else {
        return lines.iter().map(|line| theme.paint_text(line)).collect();
    };
    render_gfm_table_data(table.header, table.data_rows, theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::str_display_width;

    #[test]
    fn detects_separator_and_rows() {
        assert!(is_gfm_table_separator("|---|---|"));
        assert!(is_gfm_table_row("| A | B |"));
        assert!(!is_gfm_table_row("not a table"));
    }

    #[test]
    fn renders_box_table() {
        let lines = vec![
            "| Name | Age |".to_string(),
            "|------|-----|".to_string(),
            "| Alice | 30 |".to_string(),
        ];
        let rendered = render_gfm_pipe_table(&lines, &MarkdownTheme::dark());
        let joined = rendered.join("\n");
        assert!(joined.contains("Name"));
        assert!(joined.contains("Alice"));
        assert!(joined.contains('┌'));
        assert!(joined.contains('┘'));
    }

    #[test]
    fn display_width_ignores_ansi() {
        let cell = format!("{}{}bold{}", ansi::BOLD, ansi::fg(252), ansi::RESET);
        let padded = pad_cell(&cell, 8, "");
        assert_eq!(str_display_width(&padded), 8);
        assert!(strip_ansi(&padded).starts_with("bold"));
    }
}
