//! Terminal UI components and helpers for Elph agent applications.

pub mod agent;
pub mod bridge;
pub mod components;
pub mod diff;
pub mod prompt;
pub mod terminal;
pub mod theme;
pub mod transcript;
pub mod utils;

pub use agent::{
    AssistantMessage, AssistantMessageProps, AuthStatus, LoginDialog, LoginDialogProps, ModelSelector,
    ModelSelectorProps, OAuthSelector, OAuthSelectorProps, SessionSelector, SessionSelectorProps, ToolExecutionCard,
    ToolExecutionCardProps, ToolExecutionList, ToolExecutionListProps, TranscriptView, TranscriptViewProps,
    mock_oauth_providers, model_overlay_slot, session_overlay_slot,
};
pub use bridge::{
    DiffOverlayPortal, DiffOverlayPortalProps, OverlaySlot, OverlayStack, OverlayStackHandle,
    key_event_to_terminal_data,
};
pub use components::{Label, LabelProps, frame};
pub use diff::{
    AutocompletePopup, AutocompleteProvider, CURSOR_MARKER, CancellableLoader, ChangeType,
    CombinedAutocompleteProvider, Container as DiffContainer, CrosstermTerminal, CursorPosition, DiffLine, DiffTui,
    DiffView, Editor, EditorAction, EditorTheme, FuzzyMatch, Image, ImageOptions, ImageProtocol, ImageTheme,
    InlineSegment, InputEvent, InputResult, LineComponent, Loader, Markdown, MarkdownTheme, OverlayAnchor,
    OverlayHandle, OverlayMargin, OverlayOptions, RESET, RecordingTerminal, RenderState, SelectItem, SelectList,
    SelectListTheme, SettingItem, SettingsList, SettingsListTheme, SizeValue, SlashCommand, StdinBuffer, Terminal,
    Text, TextBlock, apply_hardware_cursor, composite_line_at, compute_side_by_side, count_added_removed,
    detect_image_protocol, do_render, encode_inline_image, extract_and_strip_cursor, find_hunk_starts,
    first_changed_line, fuzzy_filter, fuzzy_match, hardware_cursor_enabled, hyperlink, match_editor_action,
    open_tui_writer, png_dimensions, render_markdown_lines, resolve_layout,
};
pub use prompt::{
    AgentMode, ChatStream, ChatStreamProps, CollapsedPaste, DEFAULT_LINE_SCROLL_STEP, EditAction, MacEditAction,
    PAGE_SCROLL_VIEWPORT, PromptInput, PromptInputProps, PromptSegmentKind, PromptStyledSegment, PromptTranscript,
    PromptTranscriptProps, edit_action, is_force_quit_key, is_interrupt_key, is_mode_cycle_key, is_newline_key,
    is_prompt_newline_key, is_quit_command, is_submit_key, is_theme_toggle_key, mac_edit_action,
    prompt_styled_segments, reconcile_paste_offsets,
};
pub use terminal::{
    SigintReceiver, disable_keyboard_enhancement, enable_keyboard_enhancement, key_combination, sigint_channel,
};
pub use theme::{Theme, ThemeMode};
pub use transcript::{StreamingBuffer, ToolExecutionState, ToolExecutionStatus, TranscriptEntry, TranscriptRole};
pub use utils::{
    TAB_STOP, char_display_width, pad_lines, str_display_width, truncate_to_width, truncate_to_width_ellipsis,
    wrap_ansi_line, wrap_ansi_text, wrap_text,
};
