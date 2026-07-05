use super::prompt_buffer::{PromptBuffer, expand_for_display};
use super::prompt_paste::{self, CollapsedPaste, reconcile_paste_offsets};
use crate::diff::CURSOR_MARKER;
use crate::theme::Theme;
use iocraft::prelude::*;

/// Styled segment kind for prompt display (exported for integration tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptSegmentKind {
    Text,
    PasteLabel,
    PastePreview,
}

/// Byte-range segment with styling kind (exported for integration tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptStyledSegment {
    pub start: usize,
    pub end: usize,
    pub kind: PromptSegmentKind,
}

/// Returns styled byte ranges for `value`, sorted by offset.
pub fn prompt_styled_segments(value: &str, pastes: &[CollapsedPaste]) -> Vec<PromptStyledSegment> {
    styled_segments(value, pastes)
        .into_iter()
        .map(|segment| PromptStyledSegment {
            start: segment.start,
            end: segment.end,
            kind: match segment.style {
                SegmentStyle::Text => PromptSegmentKind::Text,
                SegmentStyle::PasteLabel => PromptSegmentKind::PasteLabel,
                SegmentStyle::PastePreview => PromptSegmentKind::PastePreview,
            },
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentStyle {
    Text,
    PasteLabel,
    PastePreview,
}

#[derive(Clone, Debug)]
struct StyledSegment {
    start: usize,
    end: usize,
    style: SegmentStyle,
}

struct RowRenderCtx<'a> {
    value: &'a str,
    buffer: &'a PromptBuffer,
    segments: &'a [StyledSegment],
    theme: Theme,
    text_color: Option<Color>,
    show_ime_cursor: bool,
    cursor_row: u16,
    cursor_col: u16,
}

struct ChunkRenderCtx<'a> {
    theme: Theme,
    text_color: Option<Color>,
    ime_row: bool,
    cursor_col: u16,
    row_start: usize,
    row_end: usize,
    value: &'a str,
}

#[derive(Default, Props)]
pub struct PromptDisplayProps {
    pub value: String,
    pub cursor_offset: usize,
    pub height: u16,
    pub has_focus: bool,
    pub theme: Theme,
    pub collapsed_pastes: Vec<CollapsedPaste>,
    pub measured_width: Option<State<u16>>,
    /// When true, emit [`CURSOR_MARKER`] with reverse-video cursor for IME positioning.
    pub show_hardware_cursor: bool,
}

trait UseSize<'a> {
    fn use_size(&mut self) -> (u16, u16);
}

impl<'a> UseSize<'a> for Hooks<'a, '_> {
    fn use_size(&mut self) -> (u16, u16) {
        self.use_hook(UseSizeImpl::default).size
    }
}

#[derive(Default)]
struct UseSizeImpl {
    size: (u16, u16),
}

impl Hook for UseSizeImpl {
    fn pre_component_draw(&mut self, drawer: &mut ComponentDrawer) {
        let s = drawer.size();
        self.size = (s.width, s.height);
    }
}

#[component]
pub fn PromptDisplay(mut hooks: Hooks, props: &mut PromptDisplayProps) -> impl Into<AnyElement<'static>> {
    let (width, _) = hooks.use_size();
    let Some(mut measured_width) = props.measured_width else {
        panic!("measured_width is required");
    };

    hooks.use_effect(
        move || {
            if width > 0 && measured_width.get() != width {
                measured_width.set(width);
            }
        },
        width,
    );

    let wrap_width = width.max(1).saturating_sub(1) as usize;
    let buffer = PromptBuffer::new(&props.value, wrap_width);
    let cursor = props.cursor_offset.min(props.value.len());
    let (cursor_row, mut cursor_col) = buffer.row_column_for_offset(cursor);

    if width > 0 && cursor_col >= width {
        cursor_col = width - 1;
    }

    let scroll_row = hooks.use_state(|| 0u16);
    let scroll_col = hooks.use_state(|| 0u16);
    let height = props.height.max(1);

    hooks.use_effect(
        move || {
            let mut row = scroll_row;
            let mut col = scroll_col;
            if cursor_row >= row.get() + height {
                row.set(cursor_row - height + 1);
            } else if cursor_row < row.get() {
                row.set(cursor_row);
            }
            if cursor_col >= col.get() + width {
                col.set(cursor_col - width + 1);
            } else if cursor_col < col.get() {
                col.set(cursor_col);
            }
        },
        (cursor_row, cursor_col, height, width),
    );

    let text_color = props.theme.text_color();
    let cursor_color = props.theme.input_cursor();
    let segments = styled_segments(&props.value, &props.collapsed_pastes);
    let row_children = if props.value.is_empty() {
        Vec::new()
    } else {
        row_elements(RowRenderCtx {
            value: &props.value,
            buffer: &buffer,
            segments: &segments,
            theme: props.theme,
            text_color,
            show_ime_cursor: props.has_focus && props.show_hardware_cursor,
            cursor_row,
            cursor_col,
        })
    };

    element! {
        View(
            overflow: Overflow::Hidden,
            width: 100pct,
            height: props.height,
            position: Position::Relative,
        ) {
            View(
                position: Position::Absolute,
                top: -(scroll_row.get() as i32),
                left: -(scroll_col.get() as i32),
            ) {
                #(if props.has_focus && !props.show_hardware_cursor {
                    Some(element! {
                        View(
                            position: Position::Absolute,
                            top: cursor_row,
                            left: cursor_col,
                            width: 1,
                            height: 1,
                            background_color: cursor_color,
                        )
                    })
                } else {
                    None
                })
                View(flex_direction: FlexDirection::Column) {
                    #(row_children)
                }
            }
        }
    }
}

fn row_elements(ctx: RowRenderCtx<'_>) -> Vec<AnyElement<'static>> {
    ctx.buffer
        .rows()
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let row_start = row.offset;
            let row_end = row.offset + row.len;
            let chunks = row_chunks(ctx.value, row_start, row_end, ctx.segments);
            let children = styled_chunks(
                chunks,
                ChunkRenderCtx {
                    theme: ctx.theme,
                    text_color: ctx.text_color,
                    ime_row: ctx.show_ime_cursor && row_index as u16 == ctx.cursor_row,
                    cursor_col: ctx.cursor_col,
                    row_start,
                    row_end,
                    value: ctx.value,
                },
            );
            element! {
                View(
                    flex_direction: FlexDirection::Row,
                    height: 1,
                    width: 100pct,
                    overflow: Overflow::Hidden,
                ) {
                    #(children)
                }
            }
            .into_any()
        })
        .collect()
}

fn styled_chunks(chunks: Vec<(SegmentStyle, String)>, ctx: ChunkRenderCtx<'_>) -> Vec<AnyElement<'static>> {
    if ctx.ime_row {
        let cursor_offset =
            ctx.row_start + byte_offset_for_display_col(ctx.value, ctx.row_start, ctx.row_end, ctx.cursor_col);
        let before = &ctx.value[ctx.row_start..cursor_offset.min(ctx.row_end)];
        let at = ctx
            .value
            .get(cursor_offset..)
            .and_then(|s| s.chars().next())
            .unwrap_or(' ');
        let after_start = cursor_offset + at.len_utf8();
        let after = ctx.value.get(after_start.min(ctx.row_end)..ctx.row_end).unwrap_or("");
        let mut out = Vec::new();
        if !before.is_empty() {
            out.push(element! { Text(color: ctx.text_color, content: expand_for_display(before)) }.into_any());
        }
        out.push(
            element! {
                Text(content: format!("{CURSOR_MARKER}\x1b[7m{at}\x1b[27m"))
            }
            .into_any(),
        );
        if !after.is_empty() {
            out.push(element! { Text(color: ctx.text_color, content: expand_for_display(after)) }.into_any());
        }
        return out;
    }

    chunks
        .into_iter()
        .filter(|(_, text)| !text.is_empty())
        .map(|(style, content)| {
            let content = expand_for_display(&content);
            match style {
                SegmentStyle::PasteLabel => element! {
                    Text(color: ctx.theme.paste_label(), content)
                }
                .into_any(),
                SegmentStyle::Text | SegmentStyle::PastePreview => element! {
                    Text(color: ctx.text_color, content)
                }
                .into_any(),
            }
        })
        .collect()
}

fn byte_offset_for_display_col(value: &str, row_start: usize, row_end: usize, col: u16) -> usize {
    let slice = &value[row_start..row_end];
    let mut width = 0usize;
    for (idx, ch) in slice.char_indices() {
        if width >= col as usize {
            return row_start + idx;
        }
        width += crate::utils::char_display_width(ch, width);
    }
    row_end
}

fn row_chunks(
    value: &str,
    row_start: usize,
    row_end: usize,
    segments: &[StyledSegment],
) -> Vec<(SegmentStyle, String)> {
    if row_start >= row_end {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut pos = row_start;

    for segment in segments {
        if segment.end <= row_start || segment.start >= row_end {
            continue;
        }
        let start = segment.start.max(row_start);
        let end = segment.end.min(row_end);
        if start > pos {
            chunks.push((SegmentStyle::Text, value[pos..start].to_string()));
        }
        chunks.push((segment.style, value[start..end].to_string()));
        pos = end;
    }

    if pos < row_end {
        chunks.push((SegmentStyle::Text, value[pos..row_end].to_string()));
    }

    chunks
}

fn push_marker_segments(segments: &mut Vec<StyledSegment>, base: usize, summary: &str) {
    let Some(marker) = prompt_paste::find_paste_marker_for_display(summary) else {
        segments.push(StyledSegment {
            start: base,
            end: base + summary.len(),
            style: SegmentStyle::Text,
        });
        return;
    };

    let label_start = base + marker.start;
    let label_end = label_start + marker.label.len();
    segments.push(StyledSegment {
        start: label_start,
        end: label_end,
        style: SegmentStyle::PasteLabel,
    });
    if !marker.preview.is_empty() {
        segments.push(StyledSegment {
            start: label_end,
            end: base + marker.end,
            style: SegmentStyle::PastePreview,
        });
    }
}

fn styled_segments(value: &str, pastes: &[CollapsedPaste]) -> Vec<StyledSegment> {
    if value.is_empty() {
        return Vec::new();
    }

    if !pastes.is_empty() {
        let mut resolved = pastes.to_vec();
        reconcile_paste_offsets(value, &mut resolved);

        let mut ordered: Vec<&CollapsedPaste> = resolved.iter().collect();
        ordered.sort_by_key(|paste| paste.offset);

        let mut segments = Vec::new();
        for paste in ordered {
            let start = paste.offset;
            let end = start.saturating_add(paste.summary.len());
            if end > value.len() || value[start..end] != paste.summary {
                continue;
            }
            push_marker_segments(&mut segments, start, &paste.summary);
        }
        segments.sort_by_key(|segment| segment.start);
        return segments;
    }

    let mut segments = Vec::new();
    let mut search = 0usize;
    while search < value.len() {
        let Some(marker) = prompt_paste::find_paste_marker_for_display(&value[search..]) else {
            search += 1;
            continue;
        };

        let chip_start = search + marker.start;
        let chip_end = search + marker.end;
        push_marker_segments(&mut segments, chip_start, &value[chip_start..chip_end]);
        search = chip_end;
    }

    segments.sort_by_key(|segment| segment.start);
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_paste_marker_into_styled_segments() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 3);
        let value = format!("hi {} tail", paste.summary);
        let segments = styled_segments(&value, &[paste]);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].style, SegmentStyle::PasteLabel);
        assert_eq!(segments[1].style, SegmentStyle::PastePreview);
        assert_eq!(&value[segments[0].start..segments[0].end], "[Pasted: 02 lines] ");
        assert_eq!(&value[segments[1].start..segments[1].end], "alpha");
    }

    #[test]
    fn styles_both_pastes_when_vec_order_differs_from_text_order() {
        let first = CollapsedPaste::new("alpha\nbeta".into(), 40, 0);
        let second = CollapsedPaste::new("gamma\ndelta".into(), 40, 0);
        let gap = 1usize;
        let offset_second = first.summary.len() + gap;
        let paste_first = CollapsedPaste {
            offset: offset_second,
            ..first.clone()
        };
        let paste_second = CollapsedPaste { offset: 0, ..second };
        let value = format!("{} {}", paste_second.summary, paste_first.summary);
        let segments = styled_segments(&value, &[paste_first, paste_second]);

        let labels: Vec<_> = segments
            .iter()
            .filter(|segment| segment.style == SegmentStyle::PasteLabel)
            .collect();
        let previews: Vec<_> = segments
            .iter()
            .filter(|segment| segment.style == SegmentStyle::PastePreview)
            .collect();
        assert_eq!(labels.len(), 2, "expected two paste labels, got {segments:?}");
        assert_eq!(previews.len(), 2, "expected two paste previews, got {segments:?}");
        assert_eq!(&value[previews[0].start..previews[0].end], "gamma");
        assert_eq!(&value[previews[1].start..previews[1].end], "alpha");
    }

    #[test]
    fn styles_narrow_chip_without_preview_as_label() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 18, 0);
        let value = paste.summary.clone();
        let segments = styled_segments(&value, &[paste]);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].style, SegmentStyle::PasteLabel);
        assert_eq!(&value[segments[0].start..segments[0].end], "[Pasted: 02 lines] ");
    }

    #[test]
    fn row_chunks_split_multiline_text() {
        let value = "ab\ncd";
        let buffer = PromptBuffer::new(value, 8);
        let segments = styled_segments(value, &[]);
        let rows: Vec<_> = buffer
            .rows()
            .iter()
            .map(|row| row_chunks(value, row.offset, row.offset + row.len, &segments))
            .collect();
        assert_eq!(rows[0][0].1, "ab");
        assert_eq!(rows[1][0].1, "cd");
    }
}
