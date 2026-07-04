use crate::utils::{str_display_width, truncate_to_width_no_ellipsis};

use super::ansi::{self, styled};
use super::component::{Line, LineComponent};
use super::content::{ChangeType, DiffLine, InlineSegment, compute_side_by_side, count_added_removed};

/// Side-by-side diff viewer using the Lumen-style content diff engine.
pub struct DiffView {
    old_text: String,
    new_text: String,
    tab_width: usize,
    title: Option<String>,
    cache_key: Option<(String, String, u16)>,
    cache_lines: Vec<Line>,
}

impl DiffView {
    pub fn new(old_text: impl Into<String>, new_text: impl Into<String>) -> Self {
        Self {
            old_text: old_text.into(),
            new_text: new_text.into(),
            tab_width: 8,
            title: None,
            cache_key: None,
            cache_lines: Vec::new(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn set_diff(&mut self, old_text: impl Into<String>, new_text: impl Into<String>) {
        self.old_text = old_text.into();
        self.new_text = new_text.into();
        self.invalidate();
    }

    fn build_lines(&self, width: u16) -> Vec<Line> {
        let width = width.max(1) as usize;
        let rows = compute_side_by_side(&self.old_text, &self.new_text, self.tab_width);
        let mut lines = Vec::new();

        if let Some(title) = &self.title {
            let (added, removed) = count_added_removed(&self.old_text, &self.new_text);
            let header = format!("+{added} -{removed} {title}");
            lines.push(styled(&ansi::fg(245), &header));
        }

        if rows.is_empty() {
            lines.push(styled(&ansi::fg(240), "(no changes)"));
            return lines;
        }

        let gutter = 5usize;
        let separator = 1usize;
        let half = width.saturating_sub(gutter * 2 + separator) / 2;
        if half < 4 {
            lines.extend(render_unified(&rows, width));
            return lines;
        }

        for row in &rows {
            let left = format_side_cell(row.old_line.as_ref(), row.old_segments.as_ref(), half, gutter);
            let right = format_side_cell(row.new_line.as_ref(), row.new_segments.as_ref(), half, gutter);
            let sep = styled(&ansi::fg(240), "│");
            let painted_left = paint_cell(&left, row.change_type, true);
            let painted_right = paint_cell(&right, row.change_type, false);
            lines.push(format!("{painted_left}{sep}{painted_right}"));
        }

        lines
    }
}

impl LineComponent for DiffView {
    fn render(&mut self, width: u16) -> Vec<Line> {
        let key = (self.old_text.clone(), self.new_text.clone(), width);
        if self.cache_key.as_ref() == Some(&key) {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_key = Some(key);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_key = None;
        self.cache_lines.clear();
    }
}

fn format_side_cell(
    line: Option<&(usize, String)>,
    segments: Option<&Vec<InlineSegment>>,
    half: usize,
    gutter: usize,
) -> String {
    let gutter_text = match line {
        Some((num, _)) => format!("{num:>gutter$} "),
        None => " ".repeat(gutter + 1),
    };

    let body = match (line, segments) {
        (Some(_), Some(segs)) => render_segments(segs, half.saturating_sub(gutter + 1)),
        (Some((_, text)), None) => truncate_to_width_no_ellipsis(text, half.saturating_sub(gutter + 1)),
        _ => String::new(),
    };

    let visible_budget = half.saturating_sub(str_display_width(&gutter_text));
    let clipped = if str_display_width(&body) > visible_budget {
        truncate_to_width_no_ellipsis(&body, visible_budget)
    } else {
        body
    };

    format!("{gutter_text}{clipped}")
}

fn render_segments(segments: &[InlineSegment], max_width: usize) -> String {
    let mut out = String::new();
    for segment in segments {
        let chunk = if segment.emphasized {
            styled(&format!("{}{}", ansi::fg(203), ansi::BOLD), &segment.text)
        } else {
            segment.text.clone()
        };
        if str_display_width(&out) + str_display_width(&chunk) > max_width {
            break;
        }
        out.push_str(&chunk);
    }
    out
}

fn paint_cell(text: &str, change: ChangeType, is_old: bool) -> String {
    if text.trim().is_empty() {
        return " ".repeat(str_display_width(text).max(1));
    }

    match change {
        ChangeType::Equal => text.to_string(),
        ChangeType::Insert if !is_old => styled(&ansi::fg(35), text),
        ChangeType::Delete if is_old => styled(&ansi::fg(203), text),
        ChangeType::Modified => {
            if is_old {
                styled(&ansi::fg(203), text)
            } else {
                styled(&ansi::fg(35), text)
            }
        }
        _ => styled(&ansi::fg(240), text),
    }
}

fn render_unified(rows: &[DiffLine], width: usize) -> Vec<Line> {
    let mut lines = Vec::new();
    for row in rows {
        match row.change_type {
            ChangeType::Equal => {
                if let Some((_, text)) = &row.new_line {
                    lines.push(truncate_to_width_no_ellipsis(text, width));
                }
            }
            ChangeType::Delete => {
                if let Some((num, text)) = &row.old_line {
                    let body = truncate_to_width_no_ellipsis(text, width.saturating_sub(8));
                    lines.push(styled(&ansi::fg(203), &format!("{num:>4} - {body}")));
                }
            }
            ChangeType::Insert => {
                if let Some((num, text)) = &row.new_line {
                    let body = truncate_to_width_no_ellipsis(text, width.saturating_sub(8));
                    lines.push(styled(&ansi::fg(35), &format!("{num:>4} + {body}")));
                }
            }
            ChangeType::Modified => {
                if let Some((num, text)) = &row.old_line {
                    let body = truncate_to_width_no_ellipsis(text, width.saturating_sub(8));
                    lines.push(styled(&ansi::fg(203), &format!("{num:>4} - {body}")));
                }
                if let Some((num, text)) = &row.new_line {
                    let body = truncate_to_width_no_ellipsis(text, width.saturating_sub(8));
                    lines.push(styled(&ansi::fg(35), &format!("{num:>4} + {body}")));
                }
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unified_diff_for_narrow_width() {
        let mut view = DiffView::new("a\n", "b\n");
        let lines = view.render(12);
        assert!(!lines.is_empty());
        let joined = lines.join("\n");
        assert!(joined.contains('-') || joined.contains('+'));
    }

    #[test]
    fn renders_side_by_side_for_wide_width() {
        let mut view = DiffView::new("foo\n", "bar\n");
        let lines = view.render(60);
        assert!(!lines.is_empty());
        assert!(lines[0].contains('│'));
    }
}
