---
type: Concept
title: Tools — Agent Tool System
description: Elph's agent tool system — AgentTool struct, AgentToolResult, built-in tools, feature flags, and the tool execution pipeline
tags: [tools, agent-tool, tool-execution, builtin-tools, feature-flags]
---

# Tools

The tool system lives in `crates/elph-agent/src/tools/`. It defines how tools are declared, executed, and wired into the agent loop. Tools are invoked during the [Agent Loop](../workflows/agent-loop.md) turn cycle. MCP tools are bridged through the [MCP](../domains/mcp.md) registry.

## AgentTool

Defined in `crates/elph-agent/src/tools/types.rs`:

```rust
#[derive(Clone)]
pub struct AgentTool {
    pub tool: Tool,                          // elph_ai::Tool — name, description, parameters
    pub label: String,                       // human-readable label
    pub execution_mode: Option<ToolExecutionMode>,
    pub prepare_arguments: Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>,
    pub execute: ToolExecuteFn,              // async execution callback
}
```

## AgentToolResult

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ToolResultContent>,
    pub details: Value,
    pub added_tool_names: Option<Vec<String>>,    // tools introduced by this result
    pub terminate: Option<bool>,                   // signal to end the turn
    pub usage: Option<Box<elph_ai::Usage>>,        // Sprint 5 — usage metadata
}
```

## ToolExecuteFn

```rust
pub type ToolExecuteFn = Arc<
    dyn Fn(
        String,                      // tool_call_id
        Value,                       // arguments (JSON)
        Option<CancellationToken>,   // cancellation signal
        Option<ToolUpdateCallback>,  // streaming update callback
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
        + Send + Sync
>;
```

## Built-in Tools

All gated by feature flags (from `crates/elph-agent/src/tools/mod.rs`):

| Tool                   | Module                    | Feature Flag          | Description                                       |
| ---------------------- | ------------------------- | --------------------- | ------------------------------------------------- |
| `edit_file`            | `edit_file.rs`            | `tools-edit-file`     | Apply search/replace edits                        |
| `write_file`           | `write_file.rs`           | `tools-write-file`    | Write new file content                            |
| `shell_exec`           | `shell_exec.rs`           | `tools-shell-exec`    | Execute shell commands                            |
| `read_file`            | `read_file.rs`            | `tools-read-file`     | Read file contents                                |
| `grep`                 | `grep.rs`                 | `tools-grep`          | Search file contents                              |
| `find_path`            | `find_path.rs`            | `tools-find-path`     | Find files by path                                |
| `list_dir`             | `list_dir.rs`             | `tools-list-dir`      | List directory contents                           |
| `copy_path`            | `copy_path.rs`            | `tools-copy-path`     | Copy files/directories                            |
| `create_dir`           | `create_dir.rs`           | `tools-create-dir`    | Create directories                                |
| `delete_path`          | `delete_path.rs`          | `tools-delete-path`   | Delete files/directories                          |
| `move_path`            | `move_path.rs`            | `tools-move-path`     | Move/rename files                                 |
| `web_fetch`            | `web/web_fetch.rs`        | `tools-web`           | Fetch web content                                 |
| `web_search`           | `web/web_search.rs`       | `tools-web`           | Search the web                                    |
| `web_extract`          | `web/web_extract.rs`      | `tools-web`           | Structured DOM data extraction (commit `1677e2e`) |
| `collaboration`        | `collaboration.rs`        | `tools-collaboration` | Multi-agent collaboration                         |
| `list_available_tools` | `list_available_tools.rs` | always-on             | List available tools                              |
| MCP tools              | `mcp/registry.rs`         | `mcp`                 | Dynamic MCP tool bridge                           |

## Feature Flag Groups

```toml
# Cargo.toml feature groups
builtin-tools = ["tools-edit", "tools-search", "tools-web", "tools-collaboration"]
tools-edit = ["tools-edit-file", "tools-write-file", "tools-shell-exec", "tools-create-dir", "tools-copy-path", "tools-delete-path", "tools-move-path"]
tools-search = ["tools-read-file", "tools-grep", "tools-find-path", "tools-list-dir"]
```

## Tool Execution Pipeline

1. The agent loop (`runtime/run_loop.rs`) extracts tool calls from the LLM response.
2. `execute_tool_calls()` (from `runtime/exec/execute.rs`) spawns each tool call.
3. Each tool's `execute` callback runs with the `AgentToolCall` arguments.
4. Results are collected as `ExecutedToolBatch` with `added_tool_names` and `usage`.
5. `ToolResultPatch` converts results back into `Message::ToolResult` entries.
6. Truncated messages are handled by `fail_tool_calls_from_truncated_message()`.

## BuiltinToolsBuilder

From `crates/elph-agent/src/builder.rs`:

```rust
pub struct BuiltinToolsBuilder {
    pub edit: bool,
    pub search: bool,
    pub web: bool,
    pub collaboration: bool,
    pub mcp: bool,
}

impl BuiltinToolsBuilder {
    pub fn all() -> Self;
    pub fn build(&self, env: LocalExecutionEnv) -> Vec<AgentTool>;
}
```

## Source References

- `crates/elph-agent/src/tools/types.rs` — `AgentTool`, `AgentToolResult`, `ToolExecuteFn`, `ToolResultContent`
- `crates/elph-agent/src/tools/mod.rs` — feature-gated tool modules, `BuiltinToolsBuilder` integration
- `crates/elph-agent/src/tools/list_available_tools.rs` — introspection tool
- `crates/elph-agent/src/runtime/exec/execute.rs` — `execute_tool_calls()`, `ExecutedToolBatch`
- `crates/elph-agent/src/runtime/exec/messages.rs` — tool result to message conversion
- Each tool file in `crates/elph-agent/src/tools/` — individual tool implementations
