# Built-in tools

`elph-agent` ships filesystem, shell, exploration, and web tools. Built-in tools are **optional at compile time** via Cargo features.
Register them with [`BuiltinToolsBuilder`](../src/builder.rs), group helpers, or compose your own `AgentTool` values.

## Tool groups

| Group            | Feature               | Tools                                                                                          |
| ---------------- | --------------------- | ---------------------------------------------------------------------------------------------- |
| Read & Search    | `tools-search`        | `read_file`, `grep`, `find_path`, `list_dir`                                                   |
| Edit             | `tools-edit`          | `edit_file`, `write_file`, `shell_exec`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| Terminal         | `tools-shell-use`     | `shell_use`                                                                                    |
| Web              | `tools-web`           | `web_search`, `web_fetch`, `web_extract`                                                       |
| Collaboration    | `tools-collaboration` | `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents`                    |
| Meta             | —                     | `list_available_tools` (auto-included by `BuiltinToolsBuilder`)                                |
| All of the above | `builtin-tools`       | meta feature                                                                                   |

The `elph` binary adds two additional tools not in `elph-agent`: `diagnostics` and `ask_user_question`.

## Available Tools

```
Read & Search Tools
  - list_dir     : Lists immediate children of one directory (not recursive). Prefer find_path/grep for discovery.
  - read_file    : Reads file contents; supports batch `paths`/`ranges` and streaming `offset`/`limit` windows with line numbers.
  - find_path    : Finds files by glob (`*.rs`, `**/foo.rs`) via ripgrep `--files` (gitignore-aware); falls back to fff-search.
  - grep         : Content search via system `rg` first (fast, gitignore + glob/type at search time); fff-search fallback with cached picker.
  - diagnostics  : Gets errors and warnings for either a specific file or the entire project, useful after making edits to determine if further changes are needed.

Edit Tools
  - edit_file    : Edits files by replacing specific text with new content.
  - write_file   : Creates a new file or overwrites an existing file with completely new contents.
  - shell_exec         : Executes shell commands and returns the combined output, creating a new shell process for each invocation.
  - create_dir   : Creates a new directory at the specified path within the project, creating all necessary parent directories (similar to `mkdir -p`).
  - copy_path    : Copies a file or directory recursively in the project, more efficient than manually reading and writing files when duplicating content.
  - delete_path  : Deletes a file or directory (including contents recursively) at the specified path and confirms the deletion.
  - move_path    : Moves or renames a file or directory in the project, performing a rename if only the filename differs.

Terminal Tools
  - shell_use    : Drives a real PTY terminal session (bash/zsh/fish/pwsh/cmd/nushell/...). Use for interactive programs, TUIs, REPLs, keystroke-driven prompts, and verifying on-screen state.

Session structure (host-registered; not part of BuiltinToolsBuilder)
  - todo_write   : Create/update session todos (`merge` by id, statuses pending|in_progress|completed|cancelled).
  - todo_read    : Read the current session todo list.
  - create_goal / get_goal / update_goal / set_goal_budget : Session objective + budgets (see goals module).

Web Tools
  - web_fetch    : Fetches a URL and optionally returns the content as Markdown. Useful for providing docs as context.
  - web_search   : Searches the web for information, providing results with snippets and links from relevant web pages, useful for accessing real-time information.
  - web_extract  : Extracts structured data from a web page (links, images, cleaned text, and matched elements) as JSON, using a CSS `selector` to scope a subtree. Useful for scraping/mining page structure rather than reading prose.

Collaboration Tools
  - ask_user_question  : Ask the user a question to gather structured input, then returns the user's response. It can be a single question or a structured input request.
  - spawn_agent        : Spawns a subagent with its own context window to perform a delegated task. Useful for running parallel investigations, completing self-contained tasks, or performing research where only the outcome matters.

Other Tools
  - mcp                  : Extends tools with additional MCP (Model Context Protocol) server integrations, allowing connection to external services and data sources beyond the local project.
  - skill                : Loads instructions from an available Skill so the agent can follow project-specific or workflow-specific guidance. Skills can also be invoked by you directly with slash commands.
  - list_available_tools : Lists all available tools that the agent can use, including their descriptions and usage instructions.
```

## Cargo features

| Feature               | Default | Tools / behavior                                                                               |
| --------------------- | ------- | ---------------------------------------------------------------------------------------------- |
| `builtin-tools`       | no      | Meta — enables all groups below                                                                |
| `tools-edit`          | no      | `edit_file`, `write_file`, `shell_exec`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `tools-search`        | no      | `read_file`, `grep`, `find_path`, `list_dir`                                                   |
| `tools-web`           | no      | `web_search`, `web_fetch`, `web_extract`                                                       |
| `tools-collaboration` | no      | `spawn_agent`, `send_message`, … (harness injection)                                           |
| `tools-read-file`     | no      | `read_file` only                                                                               |
| `tools-shell-exec`    | no      | `shell_exec` only                                                                              |
| `tools-shell-use`     | no      | `shell_use` only (pulls in the `shell-use` crate — in-process PTY + terminal emulator)         |
| `tools-edit-file`     | no      | `edit_file` only                                                                               |
| `tools-write-file`    | no      | `write_file` only                                                                              |
| `tools-create-dir`    | no      | `create_dir` only                                                                              |
| `tools-copy-path`     | no      | `copy_path` only                                                                               |
| `tools-delete-path`   | no      | `delete_path` only                                                                             |
| `tools-move-path`     | no      | `move_path` only                                                                               |
| `tools-grep`          | no      | `grep` only (pulls in `fff-search`)                                                            |
| `tools-find-path`     | no      | `find_path` only (pulls in `fff-search`)                                                       |
| `tools-list-dir`      | no      | `list_dir` only (pulls in `walkdir`)                                                           |
| `mcp`                 | yes     | MCP client — see [mcp.md](./mcp.md)                                                            |
| `extensions`          | yes     | WASM extension host                                                                            |
| `tracing`             | no      | `fastrace` spans + HTTP trace propagation — see [observability.md](./observability.md)         |

The `elph` binary enables `builtin-tools`, `tools-shell-use`, and `tracing` by default:

```toml
# crates/coding-agent/Cargo.toml
elph-agent = { workspace = true, features = ["tracing", "builtin-tools", "tools-shell-use"] }
```

Minimal library consumer without built-in tools:

```sh
cargo build -p elph-agent --no-default-features
```

Filesystem + web only:

```sh
cargo build -p elph-agent --no-default-features --features "tools-edit,tools-search,tools-web"
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

| Helper                       | Feature gate          | Tools                                                                                          |
| ---------------------------- | --------------------- | ---------------------------------------------------------------------------------------------- |
| `create_edit_tools`          | `tools-edit`          | `edit_file`, `write_file`, `shell_exec`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `create_search_tools`        | `tools-search`        | `read_file`, `grep`, `find_path`, `list_dir`                                                   |
| `create_all_tools`           | edit-tools/search     | all filesystem tools                                                                           |
| `create_web_tools`           | `tools-web`           | `web_search`, `web_fetch`, `web_extract`                                                       |
| `create_all_tools_with_web`  | edit-tools/search/web | filesystem + web tools                                                                         |
| `create_collaboration_tools` | `tools-collaboration` | harness-only collaboration tools                                                               |
| `create_shell_use_tool`      | `tools-shell-use`     | `shell_use` (standalone) — also included by `BuiltinToolsBuilder` when enabled                 |

```rust
use elph_agent::{BuiltinToolsBuilder, LocalExecutionEnv};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let tools = BuiltinToolsBuilder::all(env).build();
```

`echo_tool()` is always available — minimal helper for harness tests and examples.

## Execution environment

Filesystem tools resolve paths through `ExecutionEnv::absolute_path` and perform I/O through `ExecutionEnv` file and shell APIs.

`shell_use` does not use `ExecutionEnv` shell APIs. It wraps the [`shell-use`](https://crates.io/crates/shell-use) crate, which runs a `portable-pty` + alacritty terminal emulator fully in-process: each `shell_use` call executes synchronously against a process-global `SessionRegistry`. Work runs on the async runtime (the engine is bounded by per-operation internal locks and per-class timeouts).

`grep` and `find_path` resolve the search root via `ExecutionEnv`, then index and search the real filesystem under that path using [`fff-search`](https://crates.io/crates/fff-search). Indexing is synchronous and one-shot (`FilePicker::collect_files`), with `watch: false`. Work runs on a blocking thread pool so the async runtime stays responsive.

`list_dir` resolves the directory path via `ExecutionEnv`, then lists immediate children with [`walkdir`](https://crates.io/crates/walkdir) on a blocking thread pool.

`web_search` and `web_fetch` do not use `ExecutionEnv`. They perform outbound HTTP requests; HTML responses are converted to Markdown with `htmd`, and DuckDuckGo fallback search is extracted with the lightweight `astral-tl` selector engine. JavaScript-heavy pages are returned as fetched (no in-process browser).

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

Output format: `path:line:content`, one match per line. Paths are rendered relative to the working directory when possible (absolute otherwise), so results stay token-efficient while remaining actionable. Long lines are truncated to 500 characters. Overall output is capped at 50 KB.

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

The edit is applied in memory, written to disk, then **re-read from disk and compared** to the
intended result. A successful result therefore means the change actually persisted. The tool
aborts (without touching the file) if `new_string` equals `old_string` (a no-op), if the edit
would leave a standalone `old_string` **outside** the replaced region (the only allowed overlap
is an `old_string` that stays inside `new_string`, e.g. appending a tag right after its own
closing tag), if the file changed between the initial read and the write (TOCTOU — another tool
or external editor modified it), or if the on-disk content does not match after the write.

#### `write_file`

Write file contents. Creates parent directories when needed.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `path`    | string | yes      | Destination path   |
| `content` | string | yes      | Full file contents |

#### `shell_exec`

Run a shell command in the environment working directory. Output is truncated to the last 2000 lines or 50 KB.

| Parameter           | Type    | Required | Description                                                                  |
| ------------------- | ------- | -------- | ---------------------------------------------------------------------------- |
| `command`           | string  | yes      | Command to execute                                                           |
| `timeout`           | number  | no       | Timeout in seconds                                                           |
| `run_in_background` | boolean | no       | Run as a background task; returns immediately with a task id and output file |
| `disable_timeout`   | boolean | no       | Remove the timeout limit (foreground and background)                         |
| `description`       | string  | no\*     | Background task description; **required** when `run_in_background` is true   |

\* `description` is required when `run_in_background` is true. Background tasks default to a 10-minute timeout (600s) in interactive mode and no timeout in headless `elph run`; `disable_timeout` or an explicit `timeout` override this.

Each `shell_exec` run (foreground and background) persists its raw output to the session terminal directory `~/.local/share/elph/sessions/<SESSION_ID>/terminals/*.txt` (`shell-<toolCallId>.txt` for foreground, `shell-<taskId>.txt` for background). The file path is returned in `details.outputPath` and is also referenced from `tool_outputs.jsonl` (the session transcript) so output survives session resume. In stateless contexts (e.g. tests) output falls back to a temp file and `outputPath` is omitted.

**Abort / timeout semantics** — `shell_exec` runs each command as a new process group leader (`sh -c`). When the turn is aborted (Ctrl+C in the TUI) or the command times out, the **entire process group** is terminated — not just the direct shell — so grandchildren (`npm test`, `cargo build`, `sleep`, …) that hold the stdout/stderr pipes cannot keep the turn hanging. Termination is graceful (`SIGTERM`), escalated to `SIGKILL` after a short grace; the child is reaped with a bounded wait. A command whose output streams into a partial result returns the partial output with a `cancelled` flag (abort) or a timeout error.

**Background task cancellation** — background tasks are registered in a live registry keyed by `taskId` (`bg-<n>`). They can be cancelled explicitly via `elph_agent::tools::cancel_background_task(&task_id)` (terminates the process group) and enumerated via `elph_agent::tools::list_background_tasks()`. Cancellation does not happen automatically on turn abort — the task uses its own token and keeps running independently until it finishes, times out, or is cancelled explicitly. When it exits, the footer (`[exit code: …]` or an error) is appended to its output file and it is removed from the registry.

#### `shell_use`

Drive, inspect, assert on, and record real terminal sessions via a **PTY + in-process terminal emulator** (backed by the [`shell-use`](https://crates.io/crates/shell-use) crate — no external daemon or binary). Use it for interactive programs, TUIs, REPLs, and any workflow that needs keystrokes or on-screen verification; for one-shot commands prefer `shell_exec`.

One `shell_use` call maps to one `action`. Sessions are process-global and persist across calls until closed (or the process exits). The tool is classified as a mutating tool (approval + Plan-mode block like `shell_exec`).

| Parameter                                                                  | Type             | Required | Description                                                                                                                                                                          |
| -------------------------------------------------------------------------- | ---------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `action`                                                                   | string           | yes      | `open`, `run`, `submit`, `type`, `press`, `keys`, `mouse`, `resize`, `signal`, `kill`, `write`, `text`, `state`, `cells`, `get`, `screenshot`, `wait`, `expect`, `sessions`, `close` |
| `session`                                                                  | string           | no       | Session name (default `"default"`); independent sessions are keyed by name                                                                                                           |
| `shell`                                                                    | string           | no       | Shell for `open` (bash/zsh/fish/pwsh/cmd/nushell/...; default: platform)                                                                                                             |
| `cols`/`rows`                                                              | number           | no       | PTY size (default 80x30)                                                                                                                                                             |
| `cwd`                                                                      | string           | no       | Working directory for `open`/`run` (default: agent cwd)                                                                                                                              |
| `env`                                                                      | array of strings | no       | Extra `KEY=VALUE` env vars for `open`/`run`                                                                                                                                          |
| `program`/`args`                                                           | string/array     | no       | Program + args for `run`                                                                                                                                                             |
| `data`                                                                     | string           | no       | Text to `type` / `submit` / `write`                                                                                                                                                  |
| `keys`/`key`                                                               | array/string     | no       | Named keys for `press`/`keys` (`["Ctrl+C"]`, `["Escape",":","w","q","Enter"]`)                                                                                                       |
| `mouse_action`                                                             | string           | no       | `click`/`move`/`down`/`up`/`drag`/`scroll` for `mouse` (default `click`)                                                                                                             |
| `on_text`                                                                  | string           | no       | Click a visible label (`mouse click`)                                                                                                                                                |
| `x`,`y`,`w`,`h`,`x1`,`y1`,`x2`,`y2`,`button`,`clicks`,`direction`,`amount` | number           | no       | Mouse / `cells` geometry and options                                                                                                                                                 |
| `signal`                                                                   | string           | no       | `INT`/`TERM`/`KILL`/`QUIT` for `signal` (default `TERM`)                                                                                                                             |
| `field`                                                                    | string           | no       | `get` field: `command`/`output`/`exit-code`/`cwd`/`cursor`/`size`/`title`                                                                                                            |
| `kind`                                                                     | string           | no       | `wait`/`expect` kind (see below)                                                                                                                                                     |
| `text`                                                                     | string           | no       | Expected text/pattern for `wait`/`expect`                                                                                                                                            |
| `regex`/`not`/`strict`/`full`                                              | boolean          | no       | Match modifiers                                                                                                                                                                      |
| `timeout_ms`                                                               | number           | no       | Wait/expect timeout (default per-class: text/idle 5s, command/exit/ready 30s)                                                                                                        |
| `fg`/`bg`                                                                  | string           | no       | Expected color for `expect text` (`ansi-256`, `#hex`, or `default`)                                                                                                                  |
| `code`                                                                     | number           | no       | Expected exit code for `expect exit-code`                                                                                                                                            |
| `name`/`update`/`include_colors`                                           | string/boolean   | no       | `expect snapshot` options                                                                                                                                                            |
| `path`                                                                     | string           | no       | File path for `screenshot` (writes an SVG file)                                                                                                                                      |
| `all`                                                                      | boolean          | no       | `close` all sessions                                                                                                                                                                 |

**Typical workflow**

1. `open` (spawn `bash`/`zsh`/…) or `run` (spawn a program directly).
2. `submit "cmd"` → `wait` (`text`/`idle`/`command`/`exit`) → `expect` (`text`/`exit-code`/`output`/`snapshot`).
3. Inspect: `text`, `state`, `cells X Y`, `get field`, `screenshot [path]`.
4. `close` when done.

**Exit-code semantics** — assertions return a stable error class on `expect`/`wait` failure instead of raw text scraping; the tool surfaces the failure kind (`assertion`, `usage`, `no_session`, `internal`) in the message.

**Lifecycle** — sessions are process-global; `elph_agent::tools::close_shell_use_sessions()` closes them all (the `elph` binary does this on process exit via `ShellUseTeardownGuard`). `shell_use_open_sessions()` and the `sessions` action list open sessions.

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

| Parameter | Type   | Required | Default | Description                 |
| --------- | ------ | -------- | ------- | --------------------------- |
| `query`   | string | yes      | —       | Search query string         |
| `engine`  | string | no       | `auto`  | Engine selector (see below) |
| `limit`   | number | no       | `5`     | Maximum results (max: 20)   |

**Engine aliases:** `auto`, `duckduckgo` / `ddg`, `brave` / `brave-search`, `exa`, `firecrawl`, `jina` / `jina-search`, `perplexity`, `tavily`, `serpapi` / `serapi`.

Unknown `engine` values are **rejected** (they do not silently become `auto`).

#### Ranking and availability

| Mode                 | Behavior                                                                                                                                |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| **`auto`** (default) | Try configured/keyed engines by rank; DuckDuckGo HTML is last. On CAPTCHA/bot walls the error is recorded and the next engine is tried. |
| **Explicit engine**  | Use **only** that provider. No silent switch to Exa/etc. Failure returns an error naming the requested engine.                          |

DuckDuckGo uses public HTML endpoints (POST → GET → Lite). Datacenter IPs are often CAPTCHA-walled; bot walls surface as explicit errors (not “no results”). Prefer API engines (`brave`, `tavily`, `exa`, `serpapi`, `jina`) when keys are available.

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

Fetch content from a public HTTP(S) URL. HTML responses are converted to Markdown with `htmd`. Blocks private and loopback addresses (SSRF protection).

| Parameter | Type   | Required | Description                |
| --------- | ------ | -------- | -------------------------- |
| `url`     | string | yes      | HTTP or HTTPS URL to fetch |

Fetching is performed with the shared reqwest client. The response body is decoded (charset via `encoding_rs` when the `Content-Type` header declares one) and converted to Markdown by `htmd`, which skips layout and chrome tags (`script`, `style`, `nav`, `header`, `footer`, `aside`, etc.). Plain HTTP responses are returned as-is; JavaScript-heavy pages return the fetched HTML rather than a fully rendered DOM (no in-process browser).

Response bodies are capped at 256 KB. HTML is converted to Markdown; other content types are returned as-is.

#### `web_extract`

Extract **structured** data from a public HTTP(S) page as JSON — links, images, cleaned text, and matched elements — rather than converting prose to Markdown. Extraction is powered by the `astral-tl` CSS-selector engine. Blocks private and loopback addresses (SSRF protection).

| Parameter  | Type            | Required | Description                                                                                                                                                                                         |
| ---------- | --------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `url`      | string          | yes      | HTTP or HTTPS URL to extract from                                                                                                                                                                   |
| `selector` | string          | no       | CSS selector to scope extraction to a subtree (e.g. `"article"`, `".product"`, `"#main"`). When set, links/images/text are read from within that subtree and `elements` contains the matched nodes. |
| `extract`  | array of string | no       | Which data to return. Defaults to `["links", "text", "elements"]`. Allowed values: `links`, `images`, `text`, `elements`.                                                                           |
| `limit`    | number          | no       | Maximum number of links/elements/images to return. Default `100`, max `1000`.                                                                                                                       |

Links and image `src` values are resolved to absolute URLs against the page. Text is the whitespace-collapsed concatenated text of the scoped subtree(s), capped at 32 KB. The whole result is pretty-printed JSON and truncated to the same 256 KB cap as `web_fetch`.

#### Output format

```json
{
    "url": "https://example.com/page",
    "content_type": "text/html",
    "title": "Example Page",
    "selector": "#main",
    "links": [
        { "href": "https://example.com/about", "text": "About" },
        { "href": "https://example.com/contact", "text": "Contact" }
    ],
    "images": [{ "src": "https://example.com/logo.png", "alt": "Logo" }],
    "text": "Heading Some bold text here",
    "elements": [
        {
            "tag": "a",
            "attributes": { "href": "/about", "class": "link" },
            "text": "About",
            "html": "<a href=\"/about\" class=\"link\">About</a>"
        }
    ]
}
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

Lists tools the agent can **discover**, including full parameter schemas. Returns a compact XML catalog — token-cheaper than JSON (same family as `<available_skills>`). Parameter schemas flatten into `<property>` elements with `type` / `required` / `enum`; object-shaped properties recurse. Serialized with `quick-xml`.

MCP tools (`mcp_<server>__…`) are **registered** on the harness but **default-inactive** on the model-visible wire (active set) until activated. Pass optional `name_prefix` (e.g. `mcp_deepwiki__`) to:

1. Return only matching tool schemas in the XML catalog.
2. Set `added_tool_names` so the harness **lazily activates** those tools for subsequent turns.

Execution still resolves names against the full registry (`execution_tools`), so a tool that was advertised in the catalog can be invoked even if activation has not yet landed in the active set; the first MCP call also auto-activates that name. Omit `name_prefix` to browse the full catalog without activating. Automatically appended by `BuiltinToolsBuilder::build()`.

```xml
<available_tools><tool><name>read_file</name><description>Read a text or image file...</description><parameters><property name="path" type="string" required="true">File path (relative or absolute)</property><property name="limit" type="number">Maximum lines to return</property><property name="ranges" type="array of object"><description>Multiple specific file ranges to read.</description><property name="path" type="string" required="true"/><property name="offset" type="number"/></property></parameters></tool></available_tools>
```

### Other Tools

#### MCP

Extends tools with additional MCP (Model Context Protocol) server integrations. See [mcp.md](./mcp.md).

#### Skills

Loads instructions from an available Skill so the agent can follow project-specific or workflow-specific guidance. See [skills.md](./skills.md).

## Cancellation

Tool execution accepts an optional `CancellationToken`. `grep` and `find_path` bridge cancellation into `fff-search` via an abort signal polled during the blocking search. `list_dir` bridges cancellation into `walkdir` the same way.

`shell_exec` cancels by terminating its whole process group (`SIGTERM` → `SIGKILL` escalation), so multi-process commands cannot stall an abort. Exactly one `CancellationToken` is honored per run; background tasks use their own token and are never cancelled by the turn.

Compaction summarization also honors the turn's abort token when compaction runs during a busy turn — Ctrl+C stops a hung summarization call as well, instead of freezing the UI while the provider stream hangs.

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

| Test file                              | Coverage                                                      |
| -------------------------------------- | ------------------------------------------------------------- |
| `crates/elph-agent/tests/tools_fff.rs` | `grep`, `find_path`                                           |
| `crates/elph-agent/tests/web_tools.rs` | `web_search`/`web_fetch`/`web_extract` registration + ranking |
| `crates/elph-agent/tests/plan_mode.rs` | Plan mode policy and harness events                           |
| `crates/elph-agent/tests/subagent.rs`  | Subagent spawn and list                                       |

```sh
cargo test -p elph-agent --features builtin-tools --test tools_fff
cargo test -p elph-agent --features tools-web --test web_tools
cargo test -p elph-agent --features builtin-tools --test plan_mode
```

an_mode

```

```
