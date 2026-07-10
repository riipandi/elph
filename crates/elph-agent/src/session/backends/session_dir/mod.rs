//! Multi-file session directory storage.

mod chat;
mod layout;
mod storage;
mod summary;

pub use layout::{
    CHAT_HISTORY_FILE, EVENTS_FILE, PROMPT_CONTEXT_FILE, PROMPT_HISTORY_FILE, SESSION_SUBDIRS, SUMMARY_FILE,
    SYSTEM_PROMPT_FILE, UPDATES_FILE,
};
pub use storage::{SessionDirCreateOptions, SessionDirStorage, load_session_metadata};
