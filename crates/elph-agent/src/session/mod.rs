//! Tree-structured session persistence with pluggable backends.

pub mod backends;
pub mod context;
pub mod id;
pub mod migrations;
pub mod storage_utils;
pub mod tree;
pub mod types;

pub use backends::{
    InMemorySessionOptions, InMemorySessionStorage, JsonlSessionCreateOptions, JsonlSessionStorage,
    TursoSessionStorage, load_jsonl_session_metadata,
};
pub use context::build_session_context;
pub use migrations::SESSION_TREE_MIGRATIONS;
pub use tree::{BranchSummaryOptions, Session};
pub use types::{
    CustomMessageEntryBlock, CustomMessageEntryContent, JsonlSessionMetadata, SessionContext, SessionError,
    SessionErrorCode, SessionMetadata, SessionModelRef, SessionStorage, SessionTreeEntry, TursoSessionMetadata,
};
