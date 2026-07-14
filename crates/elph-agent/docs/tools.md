# Built-in tools

`elph-agent` ships filesystem, shell, exploration, and web tools. Built-in tools are **optional at compile time** via Cargo features.
Register them with [`BuiltinToolsBuilder`](../src/builder.rs), group helpers, or compose your own `AgentTool` values.

## Tool groups

| Group            | Feature               | Tools                                                                                    |
| ---------------- | --------------------- | ---------------------------------------------------------------------------------------- |
| Read & Search    | `tools-search`        | `read_file`, `grep`, `find_path`, `list_dir`                                             |
| Edit             | `tools-edit-tools`    | `edit_file`, `write_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| Web              | `tools-web`           | `web_search`, `web_fetch`                                                                |
| Collaboration    | `tools-collaboration` | `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents`              |
| Meta             | —                     | `list_available_tools` (auto-included by `BuiltinToolsBuilder`)                          |
| All of the above | `builtin-tools`       | meta feature                                                                             |

The `elph` binary adds two additional tools not in `elph-agent`: `diagnostics` and `ask_user_question`.

## Available Tools

```
Read & Search Tools
  - list_dir     : Lists files and directories in a given path, providing an overview of filesystem contents.
  - read_file    : Reads the content of a specified file in the project, allowing access to file contents.
  - find_path    : Quickly finds files by matching glob patterns (like `*/.ts`), returning matching file paths alphabetically.
  - grep         : Searches file contents across the project using regular expressions, preferred for finding symbols in code without knowing exact file paths.
  - diagnostics  : Gets errors and warnings for either a specific file or the entire project, useful after making edits to determine if further changes are needed.

Edit Tools
  - edit_file    : Edits files by replacing specific text with new content.
  - write_file   : Creates a new file or overwrites an existing file with completely new contents.
  - bash         : Executes shell commands and returns the combined output, creating a new shell process for each invocation.
  - create_dir   : Creates a new directory at the specified path within the project, creating all necessary parent directories (similar to `mkdir -p`).
  - copy_path    : Copies a file or directory recursively in the project, more efficient than manually reading and writing files when duplicating content.
  - delete_path  : Deletes a file or directory (including contents recursively) at the specified path and confirms the deletion.
  - move_path    : Moves or renames a file or directory in the project, performing a rename if only the filename differs.

Web Tools
  - web_fetch    : Fetches a URL and optionally returns the content as Markdown. Useful for providing docs as context.
  - web_search   : Searches the web for information, providing results with snippets and links from relevant web pages, useful for accessing real-time information.

Collaboration Tools
  - ask_user_question  : Ask the user a question to gather structured input, then returns the user's response. It can be a single question or a structured input request.
  - spawn_agent        : Spawns a subagent with its own context window to perform a delegated task. Useful for running parallel investigations, completing self-contained tasks, or performing research where only the outcome matters.

Other Tools
  - mcp                  : Extends tools with additional MCP (Model Context Protocol) server integrations, allowing connection to external services and data sources beyond the local project.
  - skill                : Loads instructions from an available Skill so the agent can follow project-specific or workflow-specific guidance. Skills can also be invoked by you directly with slash commands.
  - list_available_tools : Lists all available tools that the agent can use, including their descriptions and usage instructions.
```

## Cargo features

| Feature               | Default | Tools / behavior                                                                         |
| --------------------- | ------- | ---------------------------------------------------------------------------------------- |
| `builtin-tools`       | no      | Meta — enables all groups below                                                          |
| `tools-edit-tools`    | no      | `edit_file`, `write_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `tools-search`        | no      | `read_file`, `grep`, `find_path`, `list_dir`                                             |
| `tools-web`           | no      | `web_search`, `web_fetch`                                                                |
| `tools-collaboration` | no      | `spawn_agent`, `send_message`, … (harness injection)                                     |
| `tools-read-file`     | no      | `read_file` only                                                                         |
| `tools-bash`          | no      | `bash` only                                                                              |
| `tools-edit-file`     | no      | `edit_file` only                                                                         |
| `tools-write-file`    | no      | `write_file` only                                                                        |
| `tools-create-dir`    | no      | `create_dir` only                                                                        |
| `tools-copy-path`     | no      | `copy_path` only                                                                         |
| `tools-delete-path`   | no      | `delete_path` only                                                                       |
| `tools-move-path`     | no      | `move_path` only                                                                         |
| `tools-grep`          | no      | `grep` only (pulls in `fff-search`)                                                      |
| `tools-find-path`     | no      | `find_path` only (pulls in `fff-search`)                                                 |
| `tools-list-dir`      | no      | `list_dir` only (pulls in `walkdir`)                                                     |
| `mcp`                 | yes     | MCP client — see [mcp.md](./mcp.md)                                                      |
| `extensions`          | yes     | WASM extension host                                                                      |
| `obscura`             | no      | Obscura browser fallback for web tools                                                   |
| `tracing`             | no      | `fastrace` spans + HTTP trace propagation — see [observability.md](./observability.md)   |

The `elph` binary enables `builtin-tools` (and `tracing`) by default:

```toml
# elph/Cargo.toml
elph-agent = { workspace = true, features = ["tracing", "builtin-tools"] }
```

Minimal library consumer without built-in tools:

```bash
cargo build -p elph-agent --no-default-features
```

Filesystem + web only:

```bash
cargo build -p elph-agent --no-default-features --features "tools-edit-tools,tools-search,tools-web"
```

## Registration

### `BuiltinToolsBuilder` (recommended)

Assembles every tool enabled by the active Cargo features:

```rust
use elph_agent::{BuiltinToolsBuilder, LocalExecutionEnv};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));

// All compiled built-in tools (filesystem + web when tools-web is enabled)
let tools = BuiltinToolsBuilder::all(env.clone()).build();

// Filesystem tools only
let fs_tools = BuiltinToolsBuilder::new(env).without_web().build();
```

`BuiltinToolsBuilder::build()` automatically appends `list_available_tools` — a meta tool that describes all other tools in the current set.

[`AgentBuilder`](../src/builder.rs) handles app logging/init only. Use `BuiltinToolsBuilder` for the tool catalog.

### Group helpers

| Helper                       | Feature gate          | Tools                                                                                    |
| ---------------------------- | --------------------- | ---------------------------------------------------------------------------------------- |
| `create_edit_tools`          | `tools-edit-tools`    | `edit_file`, `write_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `create_search_tools`        | `tools-search`        | `read_file`, `grep`, `find_path`, `list_dir`                                             |
| `create_all_tools`           | edit-tools/search     | all filesystem tools                                                                     |
| `create_web_tools`           | `tools-web`           | `web_search`, `web_fetch`                                                                |
| `create_all_tools_with_web`  | edit-tools/search/web | filesystem + web tools                                                                   |
| `create_collaboration_tools` | `tools-collaboration` | harness-only collaboration tools                                                         |

```rust
use elph_agent::{BuiltinToolsBuilder, LocalExecutionEnv};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let tools = BuiltinToolsBuilder::all(env).build();
```

`echo_tool()` is always available — minimal helper for harness tests and examples.

## Execution environment

Filesystem tools resolve paths through `ExecutionEnv::absolute_path` and perform I/O through `ExecutionEnv` file and shell APIs.

`grep` and `find_path` resolve the search root via `ExecutionEnv`, then index and search the real filesystem under that path using [`fff-search`](https://crates.io/crates/fff-search). Indexing is synchronous and one-shot (`FilePicker::collect_files`), with `watch: false`. Work runs on a blocking thread pool so the async runtime stays responsive.

`list_dir` resolves the directory path via `ExecutionEnv`, then lists immediate children with [`walkdir`](https://crates.io/crates/walkdir) on a blocking thread pool.

`web_search` and `web_fetch` do not use `ExecutionEnv`. They perform outbound HTTP requests and optionally delegate to an Obscura browser worker thread.

## Tool reference

### Read & Search Tools

#### `read_file`

Read a text or image file. Text output is truncated to 2000 lines or 50 KB (whichever limit is hit first).

| Parameter | Type   | Required | Description                      |
| --------- | ------ | -------- | -------------------------------- |
| `path`    | string | yes      | File path (relative or absolute) |
| `offset`  | number | no       | 1-indexed start line             |
| `limit`   | number | no       | Maximum lines to return          |

#### `grep`

Search file contents under a directory or single file. Powered by `fff-search` in `FFFMode::Ai`.

| Parameter    | Type    | Required | Default | Description                              |
| ------------ | ------- | -------- | ------- | ---------------------------------------- |
| `pattern`    | string  | yes      | —       | Regex or literal search pattern          |
| `path`       | string  | no       | `.`     | Directory or file to search              |
| `literal`    | boolean | no       | `false` | Treat `pattern` as plain text, not regex |
| `ignoreCase` | boolean | no       | `false` | Case-insensitive match                   |
| `limit`      | number  | no       | `100`   | Maximum matches                          |

Output format: `absolute/path:line:content`, one match per line. Long lines are truncated to 500 characters. Overall output is capped at 50 KB.

When `path` points to a file, the search is scoped to that file via `AiGrepConfig` path constraints. When `path` is a directory, the picker indexes from that root.

`literal: true` uses plain-text mode. With `ignoreCase: true`, the pattern is escaped and searched as a case-insensitive regex.

#### `find_path`

Find files by glob pattern. Powered by `fff-search` `FilePicker::glob`.

| Parameter | Type   | Required | Default | Description               |
| --------- | ------ | -------- | ------- | ------------------------- |
| `pattern` | string | yes      | —       | Glob pattern, e.g. `*.rs` |
| `path`    | string | no       | `.`     | Directory to search       |
| `limit`   | number | no       | `1000`  | Maximum results           |

Patterns without `/` are searched recursively as `**/{pattern}`. Patterns containing `/` are matched relative to `path`. Results are relative paths, sorted alphabetically. Output is capped at 50 KB.

#### `list_dir`

List entries in a directory.

| Parameter | Type   | Required | Default | Description              |
| --------- | ------ | -------- | ------- | ------------------------ |
| `path`    | string | no       | `.`     | Directory to list        |
| `limit`   | number | no       | `1000`  | Maximum entries returned |

Directories are suffixed with `/`. Names are sorted case-insensitively.

### Edit Tools

#### `edit_file`

Replace an exact substring in a file. `old_string` must occur exactly once.

| Parameter    | Type   | Required | Description      |
| ------------ | ------ | -------- | ---------------- |
| `path`       | string | yes      | File to edit     |
| `old_string` | string | yes      | Text to replace  |
| `new_string` | string | yes      | Replacement text |

#### `write_file`

Write file contents. Creates parent directories when needed.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `path`    | string | yes      | Destination path   |
| `content` | string | yes      | Full file contents |

#### `bash`

Run a shell command in the environment working directory. Output is truncated to the last 2000 lines or 50 KB.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `command` | string | yes      | Command to execute |
| `timeout` | number | no       | Timeout in seconds |

#### `create_dir`

Create a new directory, including parent directories (like `mkdir -p`).

| Parameter | Type   | Required | Description         |
| --------- | ------ | -------- | ------------------- |
| `path`    | string | yes      | Directory to create |

#### `copy_path`

Copy a file or directory recursively.

| Parameter     | Type   | Required | Description       |
| ------------- | ------ | -------- | ----------------- |
| `source`      | string | yes      | Path to copy from |
| `destination` | string | yes      | Path to copy to   |

#### `delete_path`

Delete a file or directory recursively.

| Parameter | Type   | Required | Description    |
| --------- | ------ | -------- | -------------- |
| `path`    | string | yes      | Path to delete |

#### `move_path`

Move or rename a file or directory.

| Parameter     | Type   | Required | Description       |
| ------------- | ------ | -------- | ----------------- |
| `source`      | string | yes      | Path to move from |
| `destination` | string | yes      | Path to move to   |

### Web Tools

#### `web_search`

Search the web using multiple providers with automatic ranking and fallback.

| Parameter | Type   | Required | Default | Description                          |
| --------- | ------ | -------- | ------- | ------------------------------------ |
| `query`   | string | yes      | —       | Search query string                  |
| `engine`  | string | no       | `auto`  | Preferred engine (see aliases below) |
| `limit`   | number | no       | `5`     | Maximum results (max: 20)            |

**Engine aliases:** `auto`, `duckduckgo` / `ddg`, `brave` / `brave-search`, `exa`, `firecrawl`, `jina` / `jina-search`, `perplexity`, `tavily`, `serpapi` / `serapi`.

#### Ranking and availability

Auto mode picks the highest-ranked configured engine. DuckDuckGo is always tried last as a fallback. When all HTTP engines fail and the `obscura` feature is enabled, Obscura scrapes DuckDuckGo via a headless browser.

| Rank | Engine     | Env var                | Key required |
| ---- | ---------- | ---------------------- | ------------ |
| 1    | DuckDuckGo | —                      | no           |
| 2    | Jina       | `JINA_API_KEY`         | no           |
| 3    | Brave      | `BRAVE_SEARCH_API_KEY` | yes          |
| 4    | SerpAPI    | `SERPAPI_KEY`          | yes          |
| 5    | Tavily     | `TAVILY_API_KEY`       | yes          |
| 6    | FireCrawl  | `FIRECRAWL_API_KEY`    | no (keyless) |
| 7    | Perplexity | `PERPLEXITY_API_KEY`   | yes          |
| 8    | Exa        | `EXA_API_KEY`          | yes          |

Each provider is implemented in its own module under `src/tools/web/engines/` (`duckduckgo.rs`, `brave.rs`, etc.) for maintainability.

#### Output format

```
engine: tavily
query: rust async runtime
results: 3

1. Async programming in Rust
   url: https://rust-lang.github.io/async-book/
   snippet: Asynchronous programming in Rust using async/await.

2. Tokio
   url: https://tokio.rs/
   snippet: A runtime for writing reliable network applications.
```

#### `web_fetch`

Fetch content from a public HTTP(S) URL. HTML responses are converted to Markdown using [`htmd`](https://crates.io/crates/htmd). Blocks private and loopback addresses (SSRF protection).

| Parameter | Type   | Required | Description                |
| --------- | ------ | -------- | -------------------------- |
| `url`     | string | yes      | HTTP or HTTPS URL to fetch |

HTTP fetch is attempted first via `reqwest`. When that fails and the `obscura` feature is enabled, Obscura navigates to the page on a dedicated browser worker thread (`crossbeam-channel` + `tokio`), then extracts content from the rendered DOM.

Response bodies are capped at 256 KB. HTML is converted to Markdown; other content types are returned as-is.

#### Output format

```
url: https://example.com
content_type: text/html

# Example Domain

This domain is for use in illustrative examples in documents.
```

### Collaboration Tools

#### `spawn_agent`

Spawn a subagent with its own context window to perform a delegated task.

| Parameter   | Type   | Required | Description                       |
| ----------- | ------ | -------- | --------------------------------- |
| `task_name` | string | yes      | Short label for the subagent task |
| `message`   | string | no       | Optional initial instruction      |

#### `send_message`

Queue a message on a subagent without starting a turn.

| Parameter  | Type   | Required | Description        |
| ---------- | ------ | -------- | ------------------ |
| `agent_id` | string | yes      | Target subagent id |
| `message`  | string | yes      | Message to queue   |

#### `followup_task`

Send a message to a subagent and run a turn.

| Parameter  | Type   | Required | Description        |
| ---------- | ------ | -------- | ------------------ |
| `agent_id` | string | yes      | Target subagent id |
| `message`  | string | yes      | Message to send    |

#### `wait_agent`

Wait until a subagent finishes its current turn.

| Parameter  | Type   | Required | Description        |
| ---------- | ------ | -------- | ------------------ |
| `agent_id` | string | yes      | Target subagent id |

#### `list_agents`

List active subagents in this session. Takes no parameters.

### Meta Tools

#### `list_available_tools`

Lists all available tools that the agent can use, including their descriptions and usage instructions. Takes no parameters. Returns a JSON array of tool descriptors with `name`, `description`, `parameters`, and `required` fields.

Automatically appended by `BuiltinToolsBuilder::build()`.

### Other Tools

#### MCP

Extends tools with additional MCP (Model Context Protocol) server integrations. See [mcp.md](./mcp.md).

#### Skills

Loads instructions from an available Skill so the agent can follow project-specific or workflow-specific guidance. See [skills.md](./skills.md).

## Cancellation

Tool execution accepts an optional `CancellationToken`. `grep` and `find_path` bridge cancellation into `fff-search` via an abort signal polled during the blocking search. `list_dir` bridges cancellation into `walkdir` the same way.

## Custom tools

Use `simple_tool` for straightforward handlers or construct `AgentTool` directly when you need `prepare_arguments`, per-tool `execution_mode`, or streaming `on_update` callbacks.

Return `Err(...)` for tool failures — do not encode errors as successful text content. The agent reports thrown errors to the model as tool errors.

See the [README](../README.md#tools) for a minimal custom-tool example.

## Examples

| Example                  | Command                                                                         |
| ------------------------ | ------------------------------------------------------------------------------- |
| Faux provider smoke test | `cargo run -p elph-agent --example basic_agent`                                 |
| Coding tools             | `cargo run -p elph-agent --features builtin-tools --example agent_coding_tools` |
| Web tools                | `cargo run -p elph-agent --features builtin-tools --example agent_web_tools`    |

## Tests

| Test file                              | Coverage                            |
| -------------------------------------- | ----------------------------------- |
| `crates/elph-agent/tests/tools_fff.rs` | `grep`, `find_path`                 |
| `crates/elph-agent/tests/web_tools.rs` | `web_search` ranking, `web_fetch`   |
| `crates/elph-agent/tests/plan_mode.rs` | Plan mode policy and harness events |
| `crates/elph-agent/tests/subagent.rs`  | Subagent spawn and list             |

```bash
cargo test -p elph-agent --features builtin-tools --test tools_fff
cargo test -p elph-agent --features tools-web --test web_tools
cargo test -p elph-agent --features builtin-tools --test plan_mode
```
