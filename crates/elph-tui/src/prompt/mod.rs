mod agent_mode;
mod chat_stream;
mod editing;
mod prompt_keys;
mod queue;
mod slash_palette;
mod thinking_level;
mod transcript_scroll;
mod widget;

pub use agent_mode::AgentMode;
pub use chat_stream::{
    ChatStreamState, DEFAULT_LINE_SCROLL_STEP, PAGE_SCROLL_VIEWPORT, TranscriptStyle, render_chat_stream,
    render_chat_stream_with_agent,
};
pub use prompt_keys::{EnterAction, consume_enter_action, is_quit_command, should_cycle_agent_mode};
pub use queue::PromptQueue;
pub use slash_palette::{
    SlashPaletteAction, SlashPaletteState, elph_builtin_commands, handle_slash_palette_keys, owly_builtin_commands,
    render_slash_palette, slash_palette_visible,
};
pub use thinking_level::ThinkingLevel;
pub use transcript_scroll::{
    ScrollSnapshot, apply_transcript_auto_scroll, handle_transcript_scroll_keys, is_pinned_to_bottom,
    prepare_transcript_follow, scroll_to_bottom, unpin_auto_scroll_if_scrolled_up,
};
pub use widget::{
    PromptAction, PromptOpts, PromptState, detect_prompt_prefix, handle_prompt_input, render_prompt,
    strip_submit_trigger, text_with_theme,
};
