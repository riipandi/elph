use super::prompt_buffer::{PromptBuffer, expand_for_display};
use super::prompt_paste::{self, CollapsedPaste};
use crate::theme::Theme;
use iocraft::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentStyle {
    Text,
    PasteLabel,
    PastePreview,
}

#[derive(Clone)]
struct StyledSegment {
    start: usize,
    end: usize,
    style: SegmentStyle,
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
    let row_children = row_elements(&props.value, &buffer, &segments, props.theme, text_color);

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
                #(if props.has_focus {
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

fn row_elements(
    value: &str,
    buffer: &PromptBuffer,
    segments: &[StyledSegment],
    theme: Theme,
    text_color: Option<Color>,
) -> Vec<AnyElement<'static>> {
    buffer
        .rows()
        .iter()
        .map(|row| {
            let row_start = row.offset;
            let row_end = row.offset + row.len;
            let chunks = row_chunks(value, row_start, row_end, segments);
            let children = styled_chunks(chunks, theme, text_color);
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

fn styled_chunks(
    chunks: Vec<(SegmentStyle, String)>,
    theme: Theme,
    text_color: Option<Color>,
) -> Vec<AnyElement<'static>> {
    chunks
        .into_iter()
        .filter(|(_, text)| !text.is_empty())
        .map(|(style, content)| {
            let content = expand_for_display(&content);
            match style {
                SegmentStyle::PasteLabel => element! {
                    Text(color: theme.paste_label(), content)
                }
                .into_any(),
                SegmentStyle::Text | SegmentStyle::PastePreview => element! {
                    Text(color: text_color, content)
                }
                .into_any(),
            }
        })
        .collect()
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

fn styled_segments(value: &str, pastes: &[CollapsedPaste]) -> Vec<StyledSegment> {
    if value.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut rest_start = 0usize;
    let mut rest = value;

    for paste in pastes {
        let Some(local_start) = rest.find(&paste.summary) else {
            continue;
        };
        let start = rest_start + local_start;
        let end = start + paste.summary.len();

        if let Some(marker) = prompt_paste::find_paste_marker_for_display(&paste.summary) {
            segments.push(StyledSegment {
                start: start + marker.start,
                end: start + marker.start + marker.label.len(),
                style: SegmentStyle::PasteLabel,
            });
            if !marker.preview.is_empty() {
                segments.push(StyledSegment {
                    start: start + marker.start + marker.label.len(),
                    end,
                    style: SegmentStyle::PastePreview,
                });
            }
        } else {
            segments.push(StyledSegment {
                start,
                end,
                style: SegmentStyle::Text,
            });
        }

        rest_start = end;
        rest = &value[rest_start..];
    }

    segments.sort_by_key(|segment| segment.start);
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_paste_marker_into_styled_segments() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40);
        let value = format!("hi {} tail", paste.summary);
        let segments = styled_segments(&value, &[paste]);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].style, SegmentStyle::PasteLabel);
        assert_eq!(segments[1].style, SegmentStyle::PastePreview);
        assert_eq!(&value[segments[0].start..segments[0].end], "[Pasted: 02 lines] ");
        assert_eq!(&value[segments[1].start..segments[1].end], "alpha");
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
