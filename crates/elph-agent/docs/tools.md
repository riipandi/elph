# Built-in tools

`elph-agent` ships coding and exploration tools backed by `ExecutionEnv`. Register them via factory helpers or compose your own `AgentTool` values.

## Tool groups

| Helper                  | Tools                                      |
| ----------------------- | ------------------------------------------ |
| `create_coding_tools`   | `read`, `bash`, `edit`, `write`            |
| `create_read_only_tools`| `read`, `grep`, `find`, `ls`               |
| `create_all_tools`      | all seven built-in tools above             |

```rust
use elph_agent::{LocalExecutionEnv, create_all_tools};
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
let tools = create_all_tools(env);
```

`echo_tool()` is a minimal helper for harness tests and examples.

## Execution environment

Most tools resolve paths through `ExecutionEnv::absolute_path` and perform I/O through `ExecutionEnv` file and shell APIs.

`grep` and `find` resolve the search root via `ExecutionEnv`, then index and search the real filesystem under that path using [`fff-search`](https://crates.io/crates/fff-search). Indexing is synchronous and one-shot (`FilePicker::collect_files`), with `watch: false`. Work runs on a blocking thread pool so the async runtime stays responsive.

`ls` still lists directories through `ExecutionEnv::list_dir`.

## Tool reference

### `read`

Read a text or image file. Text output is truncated to 2000 lines or 50 KB (whichever limit is hit first).

| Parameter | Type   | Required | Description                          |
| --------- | ------ | -------- | ------------------------------------ |
| `path`    | string | yes      | File path (relative or absolute)     |
| `offset`  | number | no       | 1-indexed start line                 |
| `limit`   | number | no       | Maximum lines to return              |

### `bash`

Run a shell command in the environment working directory. Output is truncated to the last 2000 lines or 50 KB.

| Parameter | Type   | Required | Description                |
| --------- | ------ | -------- | ---------------------------- |
| `command` | string | yes      | Command to execute           |
| `timeout` | number | no       | Timeout in seconds           |

### `edit`

Replace an exact substring in a file. `old_string` must occur exactly once.

| Parameter    | Type   | Required | Description           |
| ------------ | ------ | -------- | --------------------- |
| `path`       | string | yes      | File to edit          |
| `old_string` | string | yes      | Text to replace       |
| `new_string` | string | yes      | Replacement text      |

### `write`

Write file contents. Creates parent directories when needed.

| Parameter | Type   | Required | Description        |
| --------- | ------ | -------- | ------------------ |
| `path`    | string | yes      | Destination path   |
| `content` | string | yes      | Full file contents |

### `grep`

Search file contents under a directory or single file. Powered by `fff-search` in `FFFMode::Ai`.

| Parameter    | Type    | Required | Default | Description                                      |
| ------------ | ------- | -------- | ------- | ------------------------------------------------ |
| `pattern`    | string  | yes      | —       | Regex or literal search pattern                  |
| `path`       | string  | no       | `.`     | Directory or file to search                      |
| `literal`    | boolean | no       | `false` | Treat `pattern` as plain text, not regex         |
| `ignoreCase` | boolean | no       | `false` | Case-insensitive match                           |
| `limit`      | number  | no       | `100`   | Maximum matches                                  |

Output format: `absolute/path:line:content`, one match per line. Long lines are truncated to 500 characters. Overall output is capped at 50 KB.

When `path` points to a file, the search is scoped to that file via `AiGrepConfig` path constraints. When `path` is a directory, the picker indexes from that root.

`literal: true` uses plain-text mode. With `ignoreCase: true`, the pattern is escaped and searched as a case-insensitive regex.

### `find`

Find files by glob pattern. Powered by `fff-search` `FilePicker::glob`.

| Parameter | Type   | Required | Default | Description                               |
| --------- | ------ | -------- | ------- | ----------------------------------------- |
| `pattern` | string | yes      | —       | Glob pattern, e.g. `*.rs`                 |
| `path`    | string | no       | `.`     | Directory to search                       |
| `limit`   | number | no       | `1000`  | Maximum results                           |

Patterns without `/` are searched recursively as `**/{pattern}`. Patterns containing `/` are matched relative to `path`. Results are relative paths, sorted alphabetically. Output is capped at 50 KB.

### `ls`

List entries in a directory.

| Parameter | Type   | Required | Default | Description              |
| --------- | ------ | -------- | ------- | ------------------------ |
| `path`    | string | no       | `.`     | Directory to list        |
| `limit`   | number | no       | `1000`  | Maximum entries returned |

Directories are suffixed with `/`. Names are sorted case-insensitively.

## Cancellation

Tool execution accepts an optional `CancellationToken`. `grep` and `find` bridge cancellation into `fff-search` via an abort signal polled during the blocking search.

## Custom tools

Use `simple_tool` for straightforward handlers or construct `AgentTool` directly when you need `prepare_arguments`, per-tool `execution_mode`, or streaming `on_update` callbacks.

Return `Err(...)` for tool failures — do not encode errors as successful text content. The agent reports thrown errors to the model as tool errors.

See the [README](../README.md#tools) for a minimal custom-tool example.

## Examples

| Example                     | Command                                                       |
| --------------------------- | ------------------------------------------------------------- |
| Faux provider smoke test    | `cargo run -p elph-agent --example basic_agent`               |
| OpenCode Zen via `Agent`    | `cargo run -p elph-agent --example opencode_big_pickle_agent` |

Provider-level OpenCode streaming lives in `elph-ai` as `opencode_big_pickle` (no name collision with the agent example).

## Tests

Integration tests for `grep` and `find` live in `crates/elph-agent/tests/tools_fff.rs`.

```bash
cargo test -p elph-agent --test tools_fff
```
