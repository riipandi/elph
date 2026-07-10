//! Terminal UI components and helpers for Elph agent applications.

pub mod agent;
pub mod bridge;
pub mod chrome;
pub mod components;
pub mod diff;
pub mod keys;
pub mod prompt;
pub mod shell;
pub mod terminal;
pub mod theme;
pub mod transcript;
pub mod utils;

pub use agent::{
    AuthStatus, CollapseState, ModelSelectorAction, ModelSelectorState, OAuthSelectorAction, OAuthSelectorState,
    PlanConfirmationAction, PlanConfirmationChoice as TuiPlanConfirmationChoice, PlanConfirmationState,
    SessionSelectorAction, SessionSelectorState, ToolApprovalAction, ToolApprovalState, TreeNavigatorAction,
    TreeNavigatorState, TuiToolApprovalChoice, composer_demo_entries, handle_model_selector_input,
    handle_oauth_selector_input, handle_plan_confirmation_input, handle_session_selector_input,
    handle_tool_approval_input, handle_tree_navigator_input, mock_oauth_providers, model_overlay_slot,
    render_assistant_message, render_composer_transcript, render_detail_block, render_login_dialog,
    render_model_selector, render_oauth_selector, render_pipe_message, render_plan_confirmation,
    render_session_selector, render_tool_approval, render_tool_block, render_tool_execution_card,
    render_tool_execution_list, render_transcript_view, render_tree_navigator, render_user_card, session_overlay_slot,
    tree_overlay_slot,
};
pub use bridge::{OverlaySlot, OverlayStack, key_event_to_terminal_data, render_diff_overlay};
pub use chrome::{
    ActivityState, BANNER_TIPS, BannerInfo, BannerMode, BannerState, FooterInfo, FooterMode, FooterTokenDisplay,
    StatusBarInfo, TaskItem, TaskStatus, format_tasks_completed_notice, pick_tip, render_activity, render_banner,
    render_banner_with_mode, render_footer, render_footer_with_mode, render_simple_banner, render_status_bar,
    render_tasks_panel, simple_banner_lines,
};
pub use components::{frame, inline_label_value, inline_line, render_label, text_optional_color};
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
pub use keys::{consume_ctrl_char, consume_key_code_mod, ctrl_char_for, matches_ctrl_key, pressed_ctrl_char};
pub use prompt::{
    AgentMode, ChatStreamState, DEFAULT_LINE_SCROLL_STEP, PAGE_SCROLL_VIEWPORT, PromptAction, PromptOpts, PromptQueue,
    PromptState, ScrollSnapshot, SlashPaletteAction, SlashPaletteState, ThinkingLevel, TranscriptStyle,
    apply_transcript_auto_scroll, elph_builtin_commands, handle_prompt_input, handle_slash_palette_keys,
    handle_transcript_scroll_keys, is_pinned_to_bottom, is_quit_command, owly_builtin_commands,
    prepare_transcript_follow, render_chat_stream, render_chat_stream_with_agent, render_prompt, render_slash_palette,
    scroll_to_bottom, should_cycle_agent_mode, slash_palette_visible, text_with_theme,
    unpin_auto_scroll_if_scrolled_up,
};
pub use shell::{
    OWLY_INLINE_HEIGHT, ShellChrome, ShellRegion, ShellTier, default_activity_spinner, default_run_config,
    inline_static_run_config, layout_pad, render_agent_shell, render_inline_shell,
};

pub use terminal::{SigintReceiver, disable_keyboard_enhancement, enable_keyboard_enhancement, sigint_channel};
pub use theme::{Theme, ThemeMode};
pub use transcript::{
    DEFAULT_TRANSCRIPT_CAP, StreamingBuffer, ToolExecutionState, ToolExecutionStatus, TranscriptEntry, TranscriptRole,
    cap_entries, push_capped,
};
pub use utils::{
    TAB_STOP, char_display_width, format_message_timestamp, now_timestamp, pad_lines, path_basename, read_git_branch,
    read_git_diff_stats, str_display_width, strip_ansi, truncate_to_width, truncate_to_width_ellipsis, wrap_ansi_line,
    wrap_ansi_text, wrap_text,
};
