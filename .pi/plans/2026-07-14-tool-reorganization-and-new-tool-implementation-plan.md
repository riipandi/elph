# Tool Reorganization and New Tool Implementation Plan

_Generated on 2026-07-14T13:02:48.995Z via plan mode._

plan:

---

# Tool Reorganization and New Tool Implementation Plan

## Context

The codebase currently has tools with short names (`ls`, `read`, `find`, `edit`, `write`, `webfetch`, `websearch`). The goal is to align naming with Zed's `snake_case` convention, add missing filesystem tools (`create_dir`, `copy_path`, `delete_path`, `move_path`), add a `diagnostics` tool (elph-only), and update all feature flags, policy constants, and re-exports.

## Scope

- **In scope**: `crates/elph-agent/` (tool files, mod.rs, Cargo.toml, lib.rs, builder.rs, collaboration/policy.rs) and `elph/` (runtime.rs, tool_policy.rs, new diagnostics module)
- **Out of scope**: `mcp` tool (already exists), `skill` tool (already exists via skills module), `spawn_agent` (already exists in `multi_agent.rs`)

## Current vs Target Tool Names

| Current Name | Target Name | File Rename |
|---|---|---|
| `ls` | `list_dir` | `ls.rs` → `list_dir.rs` |
| `read` | `read_file` | `read.rs` → `read_file.rs` |
| `find` | `find_path` | `find.rs` → `find_path.rs` |
| `grep` | `grep` (no change) | no change |
| `edit` | `edit_file` | `edit.rs` → `edit_file.rs` |
| `write` | `write_file` | `write.rs` → `write_file.rs` |
| `bash` | `bash` (no change) | no change |
| `webfetch` | `web_fetch` | `web/fetch.rs` → `web/web_fetch.rs` |
| `websearch` | `web_search` | `web/search.rs` → `web/web_search.rs` |
| *(new)* | `create_dir` | new `create_dir.rs` |
| *(new)* | `copy_path` | new `copy_path.rs` |
| *(new)* | `delete_path` | new `delete_path.rs` |
| *(new)* | `move_path` | new `move_path.rs` |
| *(new, elph-only)* | `diagnostics` | new `elph/src/agent/diagnostics.rs` |

## Feature Flag Renaming

| Current | Target | Reason |
|---|---|---|
| `tools-read` | `tools-read-file` | match tool name |
| `tools-find` | `tools-find-path` | match tool name |
| `tools-ls` | `tools-list-dir` | match tool name |
| `tools-explore` | `tools-search` | rename group (read+grep+find+ls) |
| `tools-core` | `tools-edit-tools` | rename group (edit+write+bash+create_dir+copy_path+delete_path+move_path) |
| `tools-grep` | `tools-grep` | no change |
| `tools-bash` | `tools-bash` | no change |
| `tools-edit` | `tools-edit-file` | match tool name |
| `tools-write` | `tools-write-file` | match tool name |
| `tools-web` | `tools-web` | no change |
| `tools-multi-agent` | `tools-multi-agent` | no change |
| `builtin-tools` | `builtin-tools` | no change |

---

## Step-by-step Plan

### Step 1 — Rename tool files and update content (elph-agent)

Rename the following files under `crates/elph-agent/src/tools/`:
- `ls.rs` → `list_dir.rs`
- `read.rs` → `read_file.rs`
- `find.rs` → `find_path.rs`
- `edit.rs` → `edit_file.rs`
- `write.rs` → `write_file.rs`
- `web/fetch.rs` → `web/web_fetch.rs`
- `web/search.rs` → `web/web_search.rs`

Inside each renamed file, update the tool name string:
- `"ls"` → `"list_dir"` (in `list_dir.rs`)
- `"read"` → `"read_file"` (in `read_file.rs`)
- `"find"` → `"find_path"` (in `find_path.rs`)
- `"edit"` → `"edit_file"` (in `edit_file.rs`)
- `"write"` → `"write_file"` (in `write_file.rs`)
- `"webfetch"` → `"web_fetch"` (in `web/web_fetch.rs`)
- `"websearch"` → `"web_search"` (in `web/web_search.rs`)

Update the label string (second arg to `simple_tool`) to match.

### Step 2 — Update `tools/mod.rs`

Update module declarations:
```rust
#[cfg(feature = "tools-list-dir")]  // was tools-ls
mod list_dir;  // was ls

#[cfg(feature = "tools-read-file")]  // was tools-read
mod read_file;  // was read

#[cfg(feature = "tools-find-path")]  // was tools-find
mod find_path;  // was find

#[cfg(feature = "tools-edit-file")]  // was tools-edit
mod edit_file;  // was edit

#[cfg(feature = "tools-write-file")]  // was tools-write
mod write_file;  // was write

#[cfg(feature = "tools-web")]
pub mod web;
```

Update `cfg` gate for `fff_picker`:
```rust
#[cfg(any(feature = "tools-grep", feature = "tools-find-path", feature = "tools-list-dir"))]
mod fff_picker;
```

Update re-exports:
```rust
pub use list_dir::create_list_dir_tool;  // was ls::create_ls_tool
pub use read_file::create_read_file_tool;
pub use find_path::create_find_path_tool;
pub use edit_file::create_edit_file_tool;
pub use write_file::create_write_file_tool;
```

Update `web/mod.rs` re-exports:
```rust
pub use web_fetch::create_web_fetch_tool;  // was fetch::create_webfetch_tool
pub use web_search::create_web_search_tool;  // was search::create_websearch_tool
```

Update group functions (`create_core_tools` → `create_edit_tools`, `create_read_only_tools` → `create_search_tools`) to use new names and feature gates.

### Step 3 — Create new filesystem tools in `elph-agent`

Create 4 new files under `crates/elph-agent/src/tools/`:

**`create_dir.rs`** — Uses `FileSystem::create_dir` from the harness. Parameters: `path` (required). Calls `ensure_parent_dir` pattern from `common.rs`.

**`copy_path.rs`** — Uses `fs::copy` / `fs_extra` or tokio fs operations via `bash` tool internals. Parameters: `source`, `destination`. Uses `resolve_path` for both. For directories, recursively copies.

**`delete_path.rs`** — Uses `FileSystem::remove` (already exists in harness as `RemoveOptions`). Parameters: `path` (required). Returns confirmation.

**`move_path.rs`** — Uses `tokio::fs::rename`. Parameters: `source`, `destination`. Uses `resolve_path` for both.

Each file follows the existing `simple_tool` pattern from `read.rs`, `edit.rs`, etc.

### Step 4 — Update `Cargo.toml` features (elph-agent)

```toml
[features]
default = ["mcp", "extensions"]
# ...
tools-read-file = []
tools-bash = []
tools-edit-file = []
tools-write-file = []
tools-create-dir = []
tools-copy-path = []
tools-delete-path = []
tools-move-path = []
tools-grep = ["dep:fff-search"]
tools-find-path = ["dep:fff-search"]
tools-list-dir = ["dep:walkdir"]
tools-web = []
tools-multi-agent = []
tools-search = ["tools-read-file", "tools-grep", "tools-find-path", "tools-list-dir"]
tools-edit-tools = ["tools-edit-file", "tools-write-file", "tools-bash", "tools-create-dir", "tools-copy-path", "tools-delete-path", "tools-move-path"]
builtin-tools = ["tools-edit-tools", "tools-search", "tools-web", "tools-multi-agent"]
```

### Step 5 — Update `builder.rs` (BuiltinToolsBuilder)

Update the `build()` method to use new tool creation function names and feature gates:
```rust
#[cfg(feature = "tools-read-file")]
crate::tools::create_read_file_tool(self.env.clone()),
#[cfg(feature = "tools-bash")]
crate::tools::create_bash_tool(self.env.clone()),
#[cfg(feature = "tools-edit-file")]
crate::tools::create_edit_file_tool(self.env.clone()),
#[cfg(feature = "tools-write-file")]
crate::tools::create_write_file_tool(self.env.clone()),
#[cfg(feature = "tools-create-dir")]
crate::tools::create_create_dir_tool(self.env.clone()),
#[cfg(feature = "tools-copy-path")]
crate::tools::create_copy_path_tool(self.env.clone()),
#[cfg(feature = "tools-delete-path")]
crate::tools::create_delete_path_tool(self.env.clone()),
#[cfg(feature = "tools-move-path")]
crate::tools::create_move_path_tool(self.env.clone()),
#[cfg(feature = "tools-grep")]
crate::tools::create_grep_tool(self.env.clone()),
#[cfg(feature = "tools-find-path")]
crate::tools::create_find_path_tool(self.env.clone()),
#[cfg(feature = "tools-list-dir")]
crate::tools::create_list_dir_tool(self.env.clone()),
```

Update group functions to match new feature gate names.

### Step 6 — Update `lib.rs` re-exports (elph-agent)

Update all `pub use` re-exports to use new names and feature gates. Key changes:
- `tools-ls` → `tools-list-dir`
- `tools-read` → `tools-read-file`
- `tools-find` → `tools-find-path`
- `tools-edit` → `tools-edit-file`
- `tools-write` → `tools-write-file`
- `tools-core` → `tools-edit-tools`
- `tools-explore` → `tools-search`
- `create_core_tools` → `create_edit_tools`
- `create_read_only_tools` → `create_search_tools`
- `create_ls_tool` → `create_list_dir_tool`
- etc.

### Step 7 — Update `collaboration/policy.rs` (elph-agent)

Update tool name constants:
```rust
const PLAN_MODE_TOOLS: &[&str] = &[
    "read_file",     // was "read"
    "grep",
    "find_path",     // was "find"
    "list_dir",      // was "ls"
    "web_fetch",     // was "webfetch"
    "web_search",    // was "websearch"
    "diagnostics",   // add for plan mode
    "ask_text",
    "ask_select",
    "ask_confirm",
];

const MUTATING_TOOLS: &[&str] = &[
    "write_file",    // was "write"
    "edit_file",     // was "edit"
    "bash",
    "create_dir",
    "copy_path",
    "delete_path",
    "move_path",
    "spawn_agent",
    "send_message",
    "followup_task",
    "wait_agent",
];
```

### Step 8 — Update `tool_policy.rs` (elph binary)

```rust
const READ_ONLY_TOOLS: &[&str] = &[
    "read_file", "grep", "find_path", "list_dir",
    "web_fetch", "web_search", "diagnostics",
];
```

### Step 9 — Implement `diagnostics` tool (elph binary only)

Create `elph/src/agent/diagnostics.rs`:
- Tool name: `diagnostics`
- Parameters: `path` (optional — specific file) 
- Implementation: Uses `bash` to run `cargo check --message-format=short` (with optional `--file` filtering), then parses and formats the output
- Read-only tool — does not mutate the workspace

Wire into `elph/src/agent/runtime.rs`: create and add to `tools` vec after `BuiltinToolsBuilder::all(env.clone()).build()`.

### Step 10 — Update tests

- Update `crates/elph-agent/tests/tools_fff.rs` for new tool names
- Update `crates/elph-agent/tests/web_tools.rs` for `web_fetch`/`web_search`
- Update `builder.rs` test assertion (`"ls"` → `"list_dir"`, `"read"` → `"read_file"`, `"websearch"` → `"web_search"`)
- Update `collaboration/policy.rs` test assertions

### Step 11 — Update examples

Update all examples under `crates/elph-agent/examples/` that reference old tool names or feature flags.

---

## Verification

1. `cargo check --features builtin-tools` — confirms all feature gates compile
2. `cargo test --features builtin-tools` — runs existing tests with new names
3. `cargo check -p elph` — confirms elph binary compiles with diagnostics
4. Manual check: tool names in the compiled agent should match the target spec

---

## Execution Order

Steps 1-2 together (rename files + update mod.rs) → Step 3 (new tools) → Step 4 (Cargo.toml) → Steps 5-8 (builder, lib, policies) → Step 9 (diagnostics) → Steps 10-11 (tests, examples) → Verification

Ready to execute on `/build`.
