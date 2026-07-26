//! Built-in agent tools and tool registry helpers.

mod common;
pub mod types;

#[cfg(feature = "tools-collaboration")]
mod collaboration;
#[cfg(feature = "tools-edit-file")]
mod edit_file;
#[cfg(any(feature = "tools-grep", feature = "tools-find-path", feature = "tools-list-dir"))]
pub mod fff_picker;
#[cfg(feature = "tools-find-path")]
mod find_path;
#[cfg(feature = "tools-grep")]
mod grep;
#[cfg(feature = "tools-list-dir")]
mod list_dir;
#[cfg(feature = "tools-read-file")]
mod read_file;
#[cfg(feature = "tools-shell-exec")]
mod shell_exec;
#[cfg(feature = "tools-web")]
pub mod web;
#[cfg(feature = "tools-write-file")]
mod write_file;

// New filesystem tools
#[cfg(feature = "tools-copy-path")]
mod copy_path;
#[cfg(feature = "tools-create-dir")]
mod create_dir;
#[cfg(feature = "tools-delete-path")]
mod delete_path;
#[cfg(feature = "tools-move-path")]
mod move_path;

mod list_available_tools;

#[cfg(feature = "mcp")]
pub mod mcp;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;

use crate::runtime::local_env::LocalExecutionEnv;
use crate::types::{AgentTool, AgentToolResult, ToolExecuteFn};

#[cfg(feature = "tools-collaboration")]
pub use collaboration::create_collaboration_tools;
#[cfg(feature = "tools-copy-path")]
pub use copy_path::create_copy_path_tool;
#[cfg(feature = "tools-create-dir")]
pub use create_dir::create_create_dir_tool;
#[cfg(feature = "tools-delete-path")]
pub use delete_path::create_delete_path_tool;
#[cfg(feature = "tools-edit-file")]
pub use edit_file::create_edit_file_tool;
#[cfg(feature = "tools-find-path")]
pub use find_path::create_find_path_tool;
#[cfg(feature = "tools-grep")]
pub use grep::create_grep_tool;
pub use list_available_tools::create_list_available_tools;
#[cfg(feature = "tools-list-dir")]
pub use list_dir::create_list_dir_tool;
#[cfg(feature = "tools-move-path")]
pub use move_path::create_move_path_tool;
#[cfg(feature = "tools-read-file")]
pub use read_file::create_read_file_tool;
#[cfg(feature = "tools-shell-exec")]
pub use shell_exec::create_shell_exec_tool;
#[cfg(feature = "tools-web")]
pub use web::{Engine as WebSearchEngine, SearchResult as WebSearchResult};
#[cfg(feature = "tools-web")]
pub use web::{create_web_fetch_tool, create_web_search_tool, create_web_tools};
#[cfg(feature = "tools-write-file")]
pub use write_file::create_write_file_tool;

pub fn simple_tool(
    tool: Tool,
    label: impl Into<String>,
    execute: impl Fn(String, Value) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
    + Send
    + Sync
    + 'static,
) -> AgentTool {
    let execute_fn: ToolExecuteFn = Arc::new(move |id, args, _signal, _on_update| execute(id, args));
    AgentTool {
        tool,
        label: label.into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: execute_fn,
    }
}

pub fn echo_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "echo".into(),
            description: "Echo text".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
        },
        "Echo",
        |_, args| {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Box::pin(async move { Ok(AgentToolResult::text(text)) })
        },
    )
}

/// Edit and filesystem mutation tools: edit_file, write_file, shell_exec, create_dir, copy_path, delete_path, move_path.
#[cfg(feature = "tools-edit")]
pub fn create_edit_tools(env: Arc<LocalExecutionEnv>) -> Vec<AgentTool> {
    let tools = vec![
        #[cfg(feature = "tools-edit-file")]
        create_edit_file_tool(env.clone()),
        #[cfg(feature = "tools-write-file")]
        create_write_file_tool(env.clone()),
        #[cfg(feature = "tools-shell-exec")]
        create_shell_exec_tool(env.clone()),
        #[cfg(feature = "tools-create-dir")]
        create_create_dir_tool(env.clone()),
        #[cfg(feature = "tools-copy-path")]
        create_copy_path_tool(env.clone()),
        #[cfg(feature = "tools-delete-path")]
        create_delete_path_tool(env.clone()),
        #[cfg(feature = "tools-move-path")]
        create_move_path_tool(env),
    ];
    tools
}

/// Read-only search and exploration tools.
#[cfg(feature = "tools-search")]
pub fn create_search_tools(env: Arc<LocalExecutionEnv>) -> Vec<AgentTool> {
    let tools = vec![
        #[cfg(feature = "tools-read-file")]
        create_read_file_tool(env.clone()),
        #[cfg(feature = "tools-grep")]
        create_grep_tool(env.clone()),
        #[cfg(feature = "tools-find-path")]
        create_find_path_tool(env.clone()),
        #[cfg(feature = "tools-list-dir")]
        create_list_dir_tool(env),
    ];
    tools
}

/// All enabled filesystem built-in tools.
#[cfg(any(feature = "tools-edit", feature = "tools-search"))]
pub fn create_all_tools(env: Arc<LocalExecutionEnv>) -> Vec<AgentTool> {
    crate::builder::BuiltinToolsBuilder::new(env).without_web().build()
}

/// All enabled built-in tools including web tools when compiled in.
#[cfg(any(feature = "tools-edit", feature = "tools-search", feature = "tools-web"))]
pub fn create_all_tools_with_web(env: Arc<LocalExecutionEnv>) -> Vec<AgentTool> {
    crate::builder::BuiltinToolsBuilder::all(env).build()
}
