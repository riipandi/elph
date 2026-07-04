mod agent_mode;
mod chat_stream;
mod paste_guard;
pub(crate) mod prompt_buffer;
mod prompt_display;
pub(crate) mod prompt_edit;
mod prompt_input;
mod prompt_keys;
pub(crate) mod prompt_paste;
mod prompt_transcript;

pub use agent_mode::AgentMode;
pub use chat_stream::{ChatStream, ChatStreamProps, DEFAULT_LINE_SCROLL_STEP, PAGE_SCROLL_VIEWPORT};
pub use prompt_input::{PromptInput, PromptInputProps};
pub use prompt_keys::{
    EditAction, MacEditAction, edit_action, is_clear_key, is_force_quit_key, is_interrupt_key, is_mode_cycle_key,
    is_mode_cycle_override_key, is_newline_key, is_pasted_tab_key, is_prompt_newline_key, is_quit_command,
    is_submit_key, is_theme_toggle_key, mac_edit_action, should_cycle_agent_mode, should_insert_tab_in_prompt,
};
pub use prompt_transcript::{PromptTranscript, PromptTranscriptProps};
