//! Terminal UI components and helpers for Elph agent applications.

pub mod agent;
pub mod chrome;
pub mod diff;
pub mod keymap;
pub mod prompt;
pub mod runtime;
pub mod shell;
pub mod terminal;
pub mod theme;
pub mod transcript;
pub mod utils;
pub mod widgets;

pub use agent::{
    AuthStatus, CollapseState, ModelSelectorAction, ModelSelectorState, OAuthSelectorAction, OAuthSelectorState,
    PlanConfirmationAction, PlanConfirmationChoice, PlanConfirmationState, SessionSelectorAction, SessionSelectorState,
    ToolApprovalAction, ToolApprovalState, TreeNavigatorAction, TreeNavigatorState, TuiToolApprovalChoice,
    mock_oauth_providers,
};
pub use chrome::{ActivityState, BANNER_TIPS, BannerInfo, BannerState, pick_tip, simple_banner_lines};
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
    open_tui_writer, png_dimensions, render_markdown_lines, render_streaming_markdown_lines, resolve_layout,
};
pub use keymap::{
    GlobalChordHandler, PromptSubmitMode, SIDEBAR_MIN_TOTAL_WIDTH, SIDEBAR_WIDTH, ShellAction, ShellActionSink,
};
pub use prompt::{
    AgentMode, ChatStreamState, PromptAction, PromptQueue, PromptState, ThinkingLevel, TranscriptStyle,
    detect_prompt_prefix, elph_builtin_commands, is_quit_command, owly_builtin_commands, strip_submit_trigger,
};
pub use runtime::{configure_runtime, start_shell};
pub use shell::{AgentShell, ShellChromeData, ShellHost};
pub use widgets::{
    CommandPaletteState, PromptPane, SidebarPlaceholder, StreamingText, TranscriptPane, build_activity_widget,
    build_footer_widget, build_palette_widget, close_palette_popup, open_palette_popup, palette_visible,
};

pub use terminal::{SigintReceiver, disable_keyboard_enhancement, enable_keyboard_enhancement, sigint_channel};
pub use theme::{Color, Theme, ThemeMode, apply_tuie_theme};
pub use transcript::{
    DEFAULT_TRANSCRIPT_CAP, StreamingBuffer, ToolExecutionState, ToolExecutionStatus, TranscriptEntry, TranscriptRole,
    cap_entries, push_capped,
};
pub use utils::{
    TAB_STOP, char_display_width, format_message_timestamp, now_timestamp, pad_lines, path_basename, read_git_branch,
    read_git_diff_stats, str_display_width, strip_ansi, truncate_to_width, truncate_to_width_ellipsis, wrap_ansi_line,
    wrap_ansi_text, wrap_text,
};
