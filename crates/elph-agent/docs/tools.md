# Built-in tools

`elph-agent` ships coding and exploration tools backed by `ExecutionEnv`, plus web tools that operate independently of the filesystem environment. Register them via factory helpers or compose your own `AgentTool` values.

## Tool groups

| Helper                      | Tools                            |
| --------------------------- | -------------------------------- |
| `create_coding_tools`       | `read`, `bash`, `edit`, `write`  |
| `create_read_only_tools`    | `read`, `grep`, `find`, `ls`     |
| `create_all_tools`          | all seven filesystem tools above |
| `create_web_tools`          | `web_search`, `web_fetch`        |
| `create_all_tools_with_web` | filesystem tools + web tools     |
| `create_multi_agent_tools`  | multi-agent tools (harness-only) |

```rust
use elph_agent::{LocalExecutionEnv, create_all_tools, create_web_tools};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let coding = create_all_tools(env);
let web = create_web_tools();
```

`echo_tool()` is a minimal helper for harness tests and examples.

## Execution environment

Filesystem tools resolve paths through `ExecutionEnv::absolute_path` and perform I/O through `ExecutionEnv` file and shell APIs.

`grep` and `find` resolve the search root via `ExecutionEnv`, then index and search the real filesystem under that path using [`fff-search`](https://crates.io/crates/fff-search). Indexing is synchronous and one-shot (`FilePicker::collect_files`), with `watch: false`. Work runs on a blocking thread pool so the async runtime stays responsive.

`ls` resolves the directory path via `ExecutionEnv`, then lists immediate children with [`walkdir`](https://crates.io/crates/walkdir) on a blocking thread pool.

`web_search` and `web_fetch` do not use `ExecutionEnv`. They perform outbound HTTP requests and optionally delegate to an Obscura browser worker thread.

## Cargo features

| Feature   | Default | Description                                            |
| --------- | ------- | ------------------------------------------------------ |
| `obscura` | yes     | Enable Obscura headless-browser fallback for web tools |

```bash
# Faster builds — HTTP-only web tools
cargo build -p elph-agent --no-default-features

# Default — includes Obscura (first build compiles V8 from source)
cargo build -p elph-agent
```

## Tool reference

### `read`

Read a text or image file. Text output is truncated to 2000 lines or 50 KB (whichever limit is hit first).

| Parameter | Type   | Required | Description                      |
| --------- | ------ | -------- | -------------------------------- |
| `path`    | string | yes      | File path (relative or absolute) |
| `offset`  | number | no       | 1-indexed start line             |
| `limit`   | number | no       | Maximum lines to return          |

### `bash`

Run a shell command in the environment working directory. Output is truncated to the last 2000 lines or 50 KB.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `command` | string | yes      | Command to execute |
| `timeout` | number | no       | Timeout in seconds |

### `edit`

Replace an exact substring in a file. `old_string` must occur exactly once.

| Parameter    | Type   | Required | Description      |
| ------------ | ------ | -------- | ---------------- |
| `path`       | string | yes      | File to edit     |
| `old_string` | string | yes      | Text to replace  |
| `new_string` | string | yes      | Replacement text |

### `write`

Write file contents. Creates parent directories when needed.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `path`    | string | yes      | Destination path   |
| `content` | string | yes      | Full file contents |

### `grep`

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

### `find`

Find files by glob pattern. Powered by `fff-search` `FilePicker::glob`.

| Parameter | Type   | Required | Default | Description               |
| --------- | ------ | -------- | ------- | ------------------------- |
| `pattern` | string | yes      | —       | Glob pattern, e.g. `*.rs` |
| `path`    | string | no       | `.`     | Directory to search       |
| `limit`   | number | no       | `1000`  | Maximum results           |

Patterns without `/` are searched recursively as `**/{pattern}`. Patterns containing `/` are matched relative to `path`. Results are relative paths, sorted alphabetically. Output is capped at 50 KB.

### `ls`

List entries in a directory.

| Parameter | Type   | Required | Default | Description              |
| --------- | ------ | -------- | ------- | ------------------------ |
| `path`    | string | no       | `.`     | Directory to list        |
| `limit`   | number | no       | `1000`  | Maximum entries returned |

Directories are suffixed with `/`. Names are sorted case-insensitively.

### `web_search`

Search the web using multiple providers with automatic ranking and fallback. Ported from [`elph-go/pkg/tools/websearch`](https://github.com/riipandi/elph-go/tree/main/pkg/tools/websearch).

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

### `web_fetch`

Fetch content from a public HTTP(S) URL. HTML responses are converted to plain text. Blocks private and loopback addresses (SSRF protection).

| Parameter | Type   | Required | Description                |
| --------- | ------ | -------- | -------------------------- |
| `url`     | string | yes      | HTTP or HTTPS URL to fetch |

HTTP fetch is attempted first via `reqwest`. When that fails and the `obscura` feature is enabled, Obscura navigates to the page on a dedicated browser worker thread (`crossbeam-channel` + `tokio`), then extracts plain text from the rendered DOM.

Response bodies are capped at 256 KB. HTML is stripped to text; other content types are returned as-is.

#### Output format

```
url: https://example.com
content_type: text/html

Example Domain
This domain is for use in illustrative examples in documents.
```

## Cancellation

Tool execution accepts an optional `CancellationToken`. `grep` and `find` bridge cancellation into `fff-search` via an abort signal polled during the blocking search. `ls` bridges cancellation into `walkdir` the same way.

## Multi-agent tools

`AgentHarness` registers these automatically via `create_multi_agent_tools` when the default active-tool set is used. They delegate to `AgentControl` and spawn child `Agent` instances with the parent model and non–multi-agent tool catalog.

| Tool            | Description                               |
| --------------- | ----------------------------------------- |
| `spawn_agent`   | Start a subagent (`task_name`, `message`) |
| `send_message`  | Queue a message without running a turn      |
| `followup_task` | Send a message and run a subagent turn      |
| `wait_agent`    | Block until the subagent is idle          |
| `list_agents`   | List id, task name, and status              |

Blocked in `CollaborationMode::Plan`. See [agent-harness.md](./agent-harness.md#collaboration-mode-and-plan-confirmation).

## Custom tools

Use `simple_tool` for straightforward handlers or construct `AgentTool` directly when you need `prepare_arguments`, per-tool `execution_mode`, or streaming `on_update` callbacks.

Return `Err(...)` for tool failures — do not encode errors as successful text content. The agent reports thrown errors to the model as tool errors.

See the [README](../README.md#tools) for a minimal custom-tool example.

## Examples

| Example                  | Command                                                       |
| ------------------------ | ------------------------------------------------------------- |
| Faux provider smoke test | `cargo run -p elph-agent --example basic_agent`               |
| OpenCode Zen via `Agent` | `cargo run -p elph-agent --example opencode_big_pickle_agent` |

Provider-level OpenCode streaming lives in `elph-ai` as `opencode_big_pickle` (no name collision with the agent example).

## Tests

| Test file                              | Coverage                          |
| -------------------------------------- | --------------------------------- |
| `crates/elph-agent/tests/tools_fff.rs` | `grep`, `find`                    |
| `crates/elph-agent/tests/web_tools.rs` | `web_search` ranking, `web_fetch` |
| `crates/elph-agent/tests/plan_mode.rs` | Plan mode policy and harness events  |
| `crates/elph-agent/tests/subagent.rs`  | Subagent spawn and list              |

```bash
cargo test -p elph-agent --test tools_fff
cargo test -p elph-agent --test web_tools
cargo test -p elph-agent --lib tools::web
```
