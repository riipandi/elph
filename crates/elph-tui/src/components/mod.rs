pub mod ascii_font;
pub mod card;
pub mod code;
pub mod dialog_shell;
pub mod diff;
pub mod frame_buffer;
pub mod input;
pub mod line_numbers;
pub mod markdown;
pub mod progress_indicator;
pub mod qr_code;
pub mod scroll_bar;
pub mod scroll_box;
pub mod select;
pub mod slider;
pub mod status_indicator;
pub mod tab_select;
pub mod text;
pub mod textarea;
pub mod theme;

pub use crate::transcript_layout::StickyHeaderLayout;
pub use crate::transcript_layout::TranscriptRowLayout;
pub use crate::transcript_layout::{
    STICKY_DEFAULT_LINE_CLAMP, STICKY_MAX_BODY_ROWS, STICKY_MAX_LINE_CLAMP, STICKY_MIN_BODY_ROWS,
};
pub use crate::transcript_layout::{
    active_sticky_user_message_index, clamp_sticky_header_rows, clamp_wrapped_transcript_lines,
};
pub use crate::transcript_layout::{
    effective_scroll_offset, layout_transcript_rows, layout_transcript_rows_widths, transcript_messages_revision,
};
pub use crate::transcript_layout::{
    latest_sticky_user_message_index, sticky_source_bubble_suppressed, sticky_user_message_index,
    transcript_bubble_inner_width, transcript_bubble_overlaps_viewport, transcript_content_row_count,
    transcript_supports_sticky_scroll, transcript_text_width, wrapped_transcript_row_count,
};
pub use crate::transcript_layout::{layout_sticky_header, scroll_viewport_height, sticky_body_line_budget};
pub use crate::transcript_layout::{
    sticky_body_line_clamp, sticky_header_display_rows, sticky_header_row_count, sticky_panel_body_cap,
};
pub use ascii_font::{AsciiText, AsciiTextProps};
pub use card::{Card, CardBorderStyle, CardProps};
pub use code::{CodeBlock, CodeBlockProps};
pub use dialog_shell::{
    ConfirmButtonAction, ConfirmButtonFocus, DIALOG_SELECT_AUTO_HEIGHT, DialogChrome, DialogConfirmButtonsContent,
    DialogConfirmButtonsContentProps, DialogConfirmContent, DialogConfirmContentProps, DialogHeader, DialogHeaderRow,
    DialogHeaderRowProps, DialogHeaderSearch, DialogHeaderSearchProps, DialogHeaderTabs, DialogHeaderTabsProps,
    DialogHeaderTitle, DialogHeaderTitleProps, DialogModeSelectContent, DialogModeSelectContentProps,
    DialogMultiChoiceContent, DialogMultiChoiceContentProps, DialogQuestionContent, DialogQuestionContentProps,
    DialogShell, DialogShellOverlay, DialogShellOverlayProps, DialogShellProps, DialogTodoListContent,
    DialogTodoListContentProps, DialogTodoProgressContent, DialogTodoProgressContentProps, DialogUserInputContent,
    DialogUserInputContentProps, MultiChoiceAction, confirm_button_key_action, dialog_body_min_height,
    dialog_choice_list_height, dialog_divider_line, dialog_header_title_fit, dialog_max_content_height,
    dialog_mode_accent, dialog_mode_from_index, dialog_mode_select_options, dialog_overlay_left, dialog_overlay_top,
    dialog_select_body_plan, dialog_select_fixed_rows, dialog_shell_estimated_height, dialog_text_rows,
    dialog_todo_list_content_rows, multi_choice_key_action, multi_choice_selected_indices, multi_choice_toggle,
    progress_row_glyph, select_list_chrome_rows, todo_row_line, todo_row_prefix,
};
pub use diff::{DiffMode, DiffView, DiffViewProps};
pub use frame_buffer::{FrameBuffer, FrameBufferView, FrameBufferViewProps};
pub use input::{Input, InputProps};
pub use line_numbers::{LineNumbers, LineNumbersProps};
pub use markdown::{
    MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTheme, MarkdownView, MarkdownViewProps, StyledSpan,
};
pub use markdown::{
    markdown_document_row_count, markdown_has_open_container_at, markdown_source_row_count, parse_markdown_document,
};
pub use markdown::{plain_text_document, render_linkified_plain_text, render_markdown_block, render_markdown_children};
pub use markdown::{render_markdown_document, render_markdown_lines, spans_with_links, streaming_tail_document};
pub use progress_indicator::{KittScannerView, KittScannerViewProps, SpinnerLoaderView, SpinnerLoaderViewProps};
pub use qr_code::{QrCodeView, QrCodeViewProps};
pub use scroll_bar::scrollbar_track_row_flags;
pub use scroll_bar::{ScrollIndicator, ScrollIndicatorProps, ScrollbarStyle};
pub use scroll_bar::{VerticalScrollbar, VerticalScrollbarProps};
pub use scroll_box::{ScrollBox, ScrollBoxProps};
pub use scroll_box::{scroll_view_down, scroll_view_max_offset, scroll_view_up};
pub use select::{
    SELECT_LIST_AUTO_HEIGHT, SelectList, SelectListProps, select_list_total_rows, select_measured_row_counts,
    select_resolve_viewport_rows,
};
pub use slider::{Slider, SliderProps};
pub use status_indicator::{
    GLYPH_ARROW_RIGHT, GLYPH_DONE, GLYPH_ELLIPSIS, GLYPH_FAILED, GLYPH_META_SEP, GLYPH_QUEUED, GLYPH_RUNNING,
    ProcessActivityTrail, ProcessActivityTrailProps, ProcessStatus, ProcessStatusIndicator,
    ProcessStatusIndicatorProps, ProcessStatusRow, ProcessStatusRowProps, process_status_color, process_status_glyph,
    process_status_word,
};
pub use tab_select::{TabSelect, TabSelectProps};
pub use text::{StyledText, StyledTextProps};
pub use textarea::{PaletteKeyInput, Textarea, TextareaLayout, TextareaProps};
pub use textarea::{
    display_row_count, layout_cursor_for_viewport, layout_textarea, logical_line_count, visible_row_count,
};
pub use theme::{
    LIST_MARKER_COL, UiTheme, UiThemeProvider, UiThemeProviderProps, list_marker, list_row_desc_style,
    list_row_name_style, resolve_ui_theme, tab_styles,
};
