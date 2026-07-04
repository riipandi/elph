//! Terminal UI components and helpers for Elph agent applications.

pub mod components;
pub mod prompt;
pub mod terminal;

pub use components::{Label, LabelProps, frame};
pub use prompt::{
    AgentMode, ChatStream, ChatStreamProps, DEFAULT_LINE_SCROLL_STEP, EditAction, MacEditAction, PAGE_SCROLL_VIEWPORT,
    PromptInput, PromptInputProps, PromptTranscript, PromptTranscriptProps, edit_action, is_interrupt_key,
    is_newline_key, is_prompt_newline_key, is_quit_command, is_submit_key, mac_edit_action,
};
pub use terminal::{disable_keyboard_enhancement, enable_keyboard_enhancement, sigint_channel};
