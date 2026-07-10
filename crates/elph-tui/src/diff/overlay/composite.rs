use crate::utils::{slice_display_columns, str_display_width, truncate_to_width_no_ellipsis};

use crate::diff::component::Line;
use crate::diff::cursor::LINE_RESET;

use super::layout::{OverlayEntry, resolve_layout};

/// Splices `overlay` into `base` at `start_col` up to `overlay_width`.
pub fn composite_line_at(
    base: &str,
    overlay: &str,
    start_col: usize,
    overlay_width: usize,
    total_width: usize,
) -> String {
    let before = slice_display_columns(base, 0, start_col);
    let after = slice_display_columns(
        base,
        start_col.saturating_add(overlay_width),
        total_width.saturating_sub(start_col.saturating_add(overlay_width)),
    );
    let clipped = truncate_to_width_no_ellipsis(overlay, overlay_width);
    let pad_before = start_col.saturating_sub(str_display_width(&before));
    let pad_overlay = overlay_width.saturating_sub(str_display_width(&clipped));
    format!("{before}{:pad_before$}{clipped}{:pad_overlay$}{after}", "", "",)
}

/// Composites visible overlays onto base lines.
pub fn composite_overlays(
    mut base_lines: Vec<Line>,
    overlays: &mut [OverlayEntry],
    term_width: u16,
    term_height: u16,
) -> Vec<Line> {
    if overlays.is_empty() {
        return base_lines;
    }

    let term_width = term_width.max(1) as usize;
    let term_height = term_height.max(1) as usize;

    let mut rendered = Vec::new();
    let mut min_lines = base_lines.len();

    let mut visible: Vec<usize> = overlays
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.is_visible(term_width as u16, term_height as u16))
        .map(|(idx, _)| idx)
        .collect();
    visible.sort_by_key(|&idx| overlays[idx].focus_order);

    for idx in visible {
        let layout = {
            let entry = &overlays[idx];
            match resolve_layout(&entry.options, 1, term_width as u16, term_height as u16) {
                Some(layout) => layout,
                None => continue,
            }
        };

        let mut overlay_lines = overlays[idx].component.render(layout.width);
        if let Some(max_h) = layout.max_height {
            overlay_lines.truncate(max_h as usize);
        }

        let layout = {
            let entry = &overlays[idx];
            resolve_layout(
                &entry.options,
                overlay_lines.len().max(1) as u16,
                term_width as u16,
                term_height as u16,
            )
            .unwrap_or(layout)
        };

        min_lines = min_lines.max(layout.row as usize + overlay_lines.len());
        rendered.push((overlay_lines, layout));
    }

    let working_height = base_lines.len().max(term_height).max(min_lines);
    while base_lines.len() < working_height {
        base_lines.push(String::new());
    }

    let viewport_start = working_height.saturating_sub(term_height);

    for (overlay_lines, layout) in rendered {
        let width = layout.width as usize;
        let col = layout.col as usize;
        for (i, overlay_line) in overlay_lines.iter().enumerate() {
            let idx = viewport_start + layout.row as usize + i;
            if idx < base_lines.len() {
                let clipped = truncate_to_width_no_ellipsis(overlay_line, width);
                base_lines[idx] = composite_line_at(&base_lines[idx], &clipped, col, width, term_width);
                if !base_lines[idx].ends_with(LINE_RESET) {
                    base_lines[idx].push_str(LINE_RESET);
                }
            }
        }
    }

    base_lines
}
