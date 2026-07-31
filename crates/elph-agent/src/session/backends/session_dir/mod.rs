//! Multi-file session directory storage.

mod chat;
mod layout;
mod storage;
mod summary;
pub mod tool_outputs;

pub use layout::CHAT_HISTORY_FILE;
pub use layout::EVENTS_FILE;
pub use layout::PROMPT_CONTEXT_FILE;
pub use layout::PROMPT_HISTORY_FILE;
pub use layout::SESSION_SUBDIRS;
pub use layout::SUMMARY_FILE;
pub use layout::SYSTEM_PROMPT_FILE;
pub use layout::UPDATES_FILE;
pub use storage::load_session_metadata;
pub use storage::{SessionDirCreateOptions, SessionDirStorage};
pub use tool_outputs::{
    ToolOutputEntry, append_tool_output, get_tool_output, load_recent_tool_outputs, load_tool_outputs,
};
