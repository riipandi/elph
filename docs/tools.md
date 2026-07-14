# Built-in Tools

Design for the agent tool catalog — permissions, provider exposure, and execution behavior.

## Compile-time catalog (`elph-agent`)

Built-in tools live in `elph-agent` and are **optional Cargo features**. The `elph` binary enables the meta feature `builtin-tools` by default so every shipped tool is available.

| Group            | Feature               | Tools                                                                                    |
| ---------------- | --------------------- | ---------------------------------------------------------------------------------------- |
| Read & Search    | `tools-search`        | `read_file`, `grep`, `find_path`, `list_dir`                                             |
| Edit             | `tools-edit-tools`    | `edit_file`, `write_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| Web              | `tools-web`           | `web_search`, `web_fetch`                                                                |
| Collaboration    | `tools-collaboration` | `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents`              |
| Meta             | —                     | `list_available_tools` (auto-included)                                                   |
| All of the above | `builtin-tools`       | meta feature                                                                             |

Registration in host apps uses [`BuiltinToolsBuilder`](../crates/elph-agent/src/builder.rs). Elph wires tools in `elph/src/agent/runtime.rs`:

```rust
let mut tools = BuiltinToolsBuilder::all(env.clone()).build();
```

Implementation reference: [`crates/elph-agent/docs/tools.md`](../crates/elph-agent/docs/tools.md).

## elph-specific tools

The `elph` binary adds two tools on top of the `elph-agent` catalog:

| Tool                | Group         | Description                                                                    |
| ------------------- | ------------- | ------------------------------------------------------------------------------ |
| `diagnostics`       | Read & Search | Gets errors and warnings for a file or the entire project (runs `cargo check`) |
| `ask_user_question` | Collaboration | Asks the user a question (text, select, or confirm mode)                       |

These tools are defined in `elph/src/agent/` and are not available in the `elph-agent` crate.

## Permission classes

| Permission          | Behavior                                                |
| ------------------- | ------------------------------------------------------- |
| `auto-allow`        | Runs without approval; user may require approval        |
| `requires-approval` | Approval dialog each run (except brave / session allow) |
| `always-approve`    | Always runs; cannot be restricted                       |

## Read & Search Tools

| Tool          | Default approval | Description                                                         |
| ------------- | ---------------- | ------------------------------------------------------------------- |
| `read_file`   | Auto-allow       | Read text/image; `offset`, `limit`; truncated to 2000 lines / 50 KB |
| `grep`        | Auto-allow       | Regex search via fff-search; content / files modes; context lines   |
| `find_path`   | Auto-allow       | Glob file search via fff-search; recursive directory listing        |
| `list_dir`    | Auto-allow       | List directory entries via walkdir; sorted, dirs suffixed with `/`  |
| `diagnostics` | Auto-allow       | `cargo check` diagnostics (elph binary only)                        |

## Edit Tools

| Tool          | Default approval  | Description                                                |
| ------------- | ----------------- | ---------------------------------------------------------- |
| `edit_file`   | Requires approval | Exact string replace; `old_string` must match exactly once |
| `write_file`  | Requires approval | Create/overwrite; creates parent dirs                      |
| `bash`        | Requires approval | Shell command in workspace; default timeout 120s, max 300s |
| `create_dir`  | Requires approval | Create directory with parents (`mkdir -p`)                 |
| `copy_path`   | Requires approval | Copy file or directory recursively                         |
| `delete_path` | Requires approval | Delete file or directory recursively                       |
| `move_path`   | Requires approval | Move or rename file or directory                           |

## Web Tools

| Tool         | Default approval | Description                                           |
| ------------ | ---------------- | ----------------------------------------------------- |
| `web_fetch`  | Auto-allow       | HTTP fetch; HTML → Markdown via htmd; SSRF protection |
| `web_search` | Auto-allow       | Multi-engine search with ranking and fallback         |

## Collaboration Tools

| Tool                | Default approval | Description                                        |
| ------------------- | ---------------- | -------------------------------------------------- |
| `ask_user_question` | Auto-allow       | Structured question to the user (elph binary only) |
| `spawn_agent`       | Auto-allow       | Start a focused subagent in an isolated context    |
| `send_message`      | Auto-allow       | Queue a message on a subagent without a turn       |
| `followup_task`     | Auto-allow       | Send a message and run a subagent turn             |
| `wait_agent`        | Auto-allow       | Block until a subagent reaches idle                |
| `list_agents`       | Auto-allow       | List subagent id, task name, and status            |

## Other Tools

| Tool    | Description                                |
| ------- | ------------------------------------------ |
| `mcp`   | Extends tools with MCP server integrations |
| `skill` | Loads instructions from an available Skill |

## Plan mode (collaboration mode)

Plan mode is a **collaboration mode**, not a pair of tools. The host application switches the harness to `CollaborationMode::Plan` (for example via `/plan` in the Elph TUI). While active:

- Only read-only exploration tools are exposed (`read_file`, `grep`, `find_path`, `list_dir`, web tools, `ask_user_question`, `diagnostics`).
- Mutating tools (`write_file`, `edit_file`, `bash`, `create_dir`, `copy_path`, `delete_path`, `move_path`) and collaboration tools (`spawn_agent`, etc.) are blocked.
- The model appends a planning system prompt and wraps the final plan in `<proposed_plan>...</proposed_plan>`.
- The harness emits `PlanProposed` and `PlanConfirmationRequired` events; the host calls `resolve_plan_confirmation()` before implementation begins.

## Goal tools

| Tool              | Description                               |
| ----------------- | ----------------------------------------- |
| `create_goal`     | Objective + optional completion criterion |
| `get_goal`        | Status, turns, tokens, budgets            |
| `update_goal`     | Lifecycle transitions                     |
| `set_goal_budget` | Token, turn, or time budget               |

## Meta tools

| Tool                   | Description                                                |
| ---------------------- | ---------------------------------------------------------- |
| `list_available_tools` | Lists all available tools with descriptions and parameters |

## Provider API exposure

Only a catalog subset is sent to the model. Exposure requires:

1. Known built-in tool
2. Approval class allowed for API (`auto-allow` or `requires-approval`)
3. Runtime can execute
4. Provider JSON schema exists

### Exposure matrix (design)

| Tool                                          | Approval | API | Runtime            |
| --------------------------------------------- | -------- | --- | ------------------ |
| read_file, grep, find_path, list_dir          | Auto     | Yes | Yes                |
| web_search, web_fetch, ask_user_question      | Auto     | Yes | Yes                |
| edit_file, write_file, bash                   | Requires | Yes | Yes (+ approval)   |
| create_dir, copy_path, delete_path, move_path | Requires | Yes | Yes (+ approval)   |
| Goal tools                                    | Auto     | Yes | Yes                |
| Collaboration tools                           | Auto     | Yes | Yes (Default mode) |

## User approval

**edit_file**, **write_file**, **bash**, **create_dir**, **copy_path**, **delete_path**, and **move_path** block the loop until the user chooses:

| Choice            | Shortcut | Effect                      |
| ----------------- | -------- | --------------------------- |
| Allow once        | `y`, `1` | This call only              |
| Allow for session | `a`, `2` | Skip approval until exit    |
| Deny              | `n`, `3` | Error returned to the model |

- **Enter** confirms default (allow once)
- **Esc** = deny
- Identical signature in the same turn → auto-deny without re-prompting
- **Brave** mode skips risky-tool approval

## WebSearch engines

Engines: DuckDuckGo (fallback), Jina, Brave, SerpAPI, Tavily, Firecrawl, Perplexity, Exa.

Omit `engine` → auto-select best configured backend; DuckDuckGo last.

Env keys: `JINA_API_KEY`, `BRAVE_SEARCH_API_KEY`, `SERPAPI_KEY`, `TAVILY_API_KEY`, `FIRECRAWL_API_KEY`, `PERPLEXITY_API_KEY`, `EXA_API_KEY`.

## Request flow

```mermaid
sequenceDiagram
    participant Session
    participant Loop as Agent loop
    participant Provider
    participant Runtime

    Session->>Loop: StartTurn
    Loop->>Provider: Complete + tool schemas
    Provider-->>Loop: tool_calls
    Loop->>Loop: Interact (AskUser / approval)
    Loop->>Runtime: Execute
    Runtime-->>Loop: output / error
    Loop->>Provider: tool_result follow-up
```

## Text markup fallback

If the model emits XML-style tags in streamed text instead of native calls:

- Parser strips markup from the visible bubble
- System prompt discourages invented tags
- Native tool calling remains the primary path

## Related

- [agent-runtime.md](./agent-runtime.md) — execution flow
- [slash-commands.md](./slash-commands.md) — diagnostics
- [tui.md](./tui.md) — tool display
