//! App-agnostic agent runtime: loop, tools, sessions, MCP, and harness.
//!
//! # Public contract
//!
//! Crate-root prelude: [`Agent`], [`AgentOptions`], [`HostIdentity`],
//! [`AgentError`], [`ToolError`], and feature-gated tool constructors.
//! Everything else is imported from its module (`harness`, [`session`],
//! [`compaction`], [`collaboration`], [`runtime`], [`mcp`], …).
//! Adapter internals and TypeScript-port helpers (`get_or_throw`,
//! `get_or_undefined`) are not a stability promise.
//!
//! Set [`HostIdentity`] on [`AgentOptions`] (env prefix for
//! `{PREFIX}_PROMPT_ENCODING*`). Pass the same prefix to MCP
//! [`mcp::load_or_create_master_key_with_prefix`] and logging [`AgentBuilder::env_prefix`].
//! Identity is not process-global.
//!
//! MSRV is Rust **1.89** (edition 2024). Cargo features: `mcp`, `builtin-tools`,
//! `extensions`, `prompt-templates`, `tracing`, `backend-turso`; bundle with `full`.
//!
//! Consumer notes: <https://github.com/riipandi/elph/blob/main/docs/elph-agent.md>
//!
//! Rust port of [@earendil-works/pi-agent](https://github.com/earendil-works/pi/tree/main/packages/agent).

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod agent;
pub mod builder;
pub mod compaction;
#[cfg(feature = "backend-turso")]
pub mod datastore;
pub mod fs;
pub mod goals;
pub mod logger;
pub mod messages;

pub mod collaboration;
pub mod exec;
#[cfg(feature = "extensions")]
pub mod plugins;
pub mod prompt;

pub mod runtime;

pub mod session;
pub mod session_summary;
pub mod skills;
pub mod todos;
pub mod turns;
pub mod workers;

pub mod tools;
pub mod trace;
pub mod types;
pub mod utils;

pub use agent::default_model;
pub use agent::harness;
pub use agent::{Agent, AgentListener, AgentOptions, AgentSubscription, PartialAgentState};
pub use builder::{AgentBuilder, AgentInit, BuiltinToolsBuilder};
pub use prompt::{DEFAULT_SYSTEM_PROMPT, resolve_system_prompt_text};
#[cfg(any(feature = "tools-edit", feature = "tools-search"))]
pub use tools::create_all_tools;
#[cfg(any(feature = "tools-edit", feature = "tools-search", feature = "tools-web"))]
pub use tools::create_all_tools_with_web;
#[cfg(feature = "tools-collaboration")]
pub use tools::create_collaboration_tools;
#[cfg(feature = "tools-copy-path")]
pub use tools::create_copy_path_tool;
#[cfg(feature = "tools-create-dir")]
pub use tools::create_create_dir_tool;
#[cfg(feature = "tools-delete-path")]
pub use tools::create_delete_path_tool;
#[cfg(feature = "tools-edit-file")]
pub use tools::create_edit_file_tool;
#[cfg(feature = "tools-edit")]
pub use tools::create_edit_tools;
#[cfg(feature = "tools-find-path")]
pub use tools::create_find_path_tool;
#[cfg(feature = "tools-grep")]
pub use tools::create_grep_tool;
pub use tools::create_list_available_tools;
#[cfg(feature = "tools-list-dir")]
pub use tools::create_list_dir_tool;
pub use tools::create_list_skills_tool;
#[cfg(feature = "tools-move-path")]
pub use tools::create_move_path_tool;
#[cfg(feature = "tools-read-file")]
pub use tools::create_read_file_tool;
#[cfg(feature = "tools-search")]
pub use tools::create_search_tools;
#[cfg(feature = "tools-write-file")]
pub use tools::create_write_file_tool;
#[cfg(feature = "mcp")]
pub use tools::mcp;
#[cfg(feature = "tools-web")]
pub use tools::{WebSearchEngine, WebSearchResult};
#[cfg(feature = "tools-shell-exec")]
pub use tools::{
    cancel_background_task, create_shell_exec_tool, list_background_tasks, normalize_shell_exec_args,
    strip_redundant_cd_prefix,
};
#[cfg(feature = "tools-shell-use")]
pub use tools::{close_shell_use_sessions, create_shell_use_tool, shell_use_open_sessions};
#[cfg(feature = "tools-web")]
pub use tools::{create_web_extract_tool, create_web_fetch_tool, create_web_search_tool, create_web_tools};
pub use tools::{echo_tool, simple_tool};
pub use types::*;
