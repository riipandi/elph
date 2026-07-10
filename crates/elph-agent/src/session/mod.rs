//! Tree-structured session persistence with pluggable backends.

pub mod backends;
pub mod context;
pub mod id;
pub mod migrations;
pub mod repo;
pub mod repo_utils;
pub mod storage_utils;
pub mod tree;
pub mod types;

pub use backends::session_dir::{EVENTS_FILE, SUMMARY_FILE};
pub use backends::{
    InMemorySessionOptions, InMemorySessionStorage, SessionDirCreateOptions, SessionDirStorage, TursoSessionStorage,
    load_session_metadata,
};
pub use context::build_session_context;
pub use migrations::SESSION_TREE_MIGRATIONS;
pub use repo::{
    InMemorySessionCreateOptions, InMemorySessionRepo, SessionDirListOptions, SessionDirRepo,
    SessionDirRepoCreateOptions,
};
pub use repo_utils::{
    ForkEntriesOptions, ForkPosition, create_session_id, create_timestamp, get_entries_to_fork, to_session,
};
pub use tree::{BranchSummaryOptions, Session};
pub use types::{
    CustomMessageEntryBlock, CustomMessageEntryContent, SessionContext, SessionDirMetadata, SessionError,
    SessionErrorCode, SessionMetadata, SessionModelRef, SessionStorage, SessionTreeEntry, TursoSessionMetadata,
};
