# Agent Runtime

Design for the path from user input to model response, tool execution, and TUI updates.

## Goals

- A single **turn** may include many tool rounds before a final reply.
- Text and thinking stream into the transcript in real time.
- Risky tools wait for user approval before running.
- Conversation history compacts automatically to stay within context limits.
- Sessions can resume after the app exits.

## Entry points

| Trigger                 | Expected behavior                                          |
| ----------------------- | ---------------------------------------------------------- |
| Normal chat input       | Start turn → tool loop → finish → persist                  |
| Prompt template `/name` | Expand template → send as user turn                        |
| `!cmd` / `!!cmd`        | Run shell; `!` may queue output for a follow-up agent turn |
| No provider configured  | Block submit or run placeholder turn                       |
| Non-interactive `run`   | One prompt → stdout → exit                                 |

## Turn cycle

```
User message
    → assemble system prompt + resources + history
    → stream completion (with tool schemas)
    → [tool call?] → approve / ask user → execute → append results
    → repeat until the model stops calling tools
    → persist history + emit turn_done
```

### Turn modes

| Condition                | Behavior                    |
| ------------------------ | --------------------------- |
| Provider + tools enabled | Native tool loop            |
| Provider, tools disabled | Single completion, no tools |
| Shell-context prompt     | Placeholder response        |
| No provider              | Placeholder phases          |

### Tool loop limit

- No host setting yet; the agent loop continues until the model stops calling tools (or errors / aborts).

## System prompt

Assembly order:

1. Elph persona and session context — working directory, date, OS, and shell
2. Registered skill metadata — the agent reads a matching skill body before acting
3. Coding instructions — rule precedence, safety, tool routing, execution, output, and language preference
4. Mode-specific constraints — Build, Brave, Plan, or Ask
5. Project context — nearest `AGENTS.md`, appended after generic instructions so scoped repository rules remain prominent
6. Optional memory context

The active tool list is rendered dynamically on every turn. Tool guidance names only tools exposed in that mode, prefers dedicated search/edit/file tools over shell workarounds, parallelizes independent calls, and reserves `list_available_tools` for unfamiliar or dynamically added tools. When collaboration tools are active, the prompt also defines a conditional subagent lifecycle: delegate only substantial isolated work, assign disjoint write scopes, reuse agents for follow-ups, wait only when results are needed, and synthesize rather than forwarding raw output.

## Tool loop

1. Send **exposed** tool schemas to the provider.
2. Receive `tool_calls` / `tool_use` from the stream.
3. Interactive tools: block until the user answers.
4. Risky tools: approval dialog (unless brave / allow-for-session).
5. Execute; stream shell output to the TUI when applicable.
6. Optionally rewrite structured tool output as [TOON](https://github.com/toon-format/toon) before the model sees it (`settings.json` → `promptEncoding`, `ELPH_PROMPT_ENCODING`, or built-in default `off`).
7. Append assistant + tool result messages to history.
8. Repeat until no tool calls remain.

### TOON prompt encoding (optional)

When enabled, the agent runtime may compress large JSON tool results (and MCP `structured_content`) into TOON fenced blocks in model-visible `content`. This reduces input tokens on tabular payloads; wire/API JSON is unchanged.

| Mode   | Behavior                                      |
| ------ | --------------------------------------------- |
| `off`  | Default — tool results pass through unchanged |
| `toon` | Encode eligible JSON ≥ size threshold         |
| `auto` | Encode only uniform tabular JSON arrays       |

**Configuration** — precedence (highest first):

1. `settings.json` → `promptEncoding` group (host maps it into harness options; subagents inherit it). `null`/absent = skip to env.
2. `ELPH_PROMPT_ENCODING`, `ELPH_PROMPT_ENCODING_MIN_BYTES`, `ELPH_PROMPT_ENCODING_DELIMITER`, `ELPH_PROMPT_ENCODING_TABULAR_DELIMITER` env vars.
3. Built-in default (`off`, `minBytes` 2048).

Implementation and examples: [`elph-agent` prompt-encoding.md](../crates/elph-agent/docs/prompt-encoding.md).

### Exposure layers

| Layer        | Role                                          |
| ------------ | --------------------------------------------- |
| Catalog      | Full built-in list (UI, prompts, diagnostics) |
| Provider API | Subset with JSON schemas                      |
| Runtime      | Tools that can actually execute               |

A tool is sent to the API only if it is known, has a schema, is executable, and matches the exposure policy for its approval class.

## History compaction

| Limit                        | Design value |
| ---------------------------- | ------------ |
| Max messages                 | 32           |
| Max total size               | ~512 KB      |
| Max tool result (API)        | 32 KB        |
| Max tool result (TUI detail) | 40 KB        |
| Max assistant message        | 64 KB        |
| Max TUI bubble               | 48 KB        |

### Compaction

Configured via `settings.compaction` (`thresholdPct`, `keepRecentTokens`) mapped into harness `CompactionSettings` (auto-compact is always enabled from the host). Summarization can use `models.compactionModel` (`inherit` = session model).

| Path                    | Behavior                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Auto**                | If context usage exceeds the threshold — checked **before** sending a new prompt (including the upcoming prompt size) and **after** every turn (successful or errored) — Elph compacts history and posts sticky transcript notices (will / running / done or failed). The estimate matches the header context label (session messages + compiled system prompt, counted once), so `thresholdPct` fires exactly when the chrome shows that percentage. The system prompt is only added on top of the message estimate when provider usage is not reused, so the label never double-counts it and cannot read above 100% of the window for a request that actually fits. When a turn errors with a context-limit overflow, Elph compacts and retries once, but only if the retry still fits after compaction. |
| **Turn-error recovery** | When a turn ends in a provider error that indicates a context-limit overflow (or usage is already over threshold), Elph compacts automatically and then submits a **Continue-style recovery prompt once** (not the original text), so the interrupted task resumes without duplicating tool calls that already succeeded. The retry is bounded to a single attempt.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| **Manual**              | `/compact` (or `/c`) — same lifecycle notices; noop when nothing to summarize.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| **Model switch**        | Switching to a **smaller** context window checks whether history still fits; if not, compact (up to two passes) with notices before the next turn.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

Manual and auto share the harness compact path (with optional compaction-model override so the session footer model is not permanently changed).

## Agent events → TUI

| Event             | TUI effect                                   |
| ----------------- | -------------------------------------------- |
| Activity          | Working label + elapsed time                 |
| Thinking delta    | Append to thinking block                     |
| Response delta    | Append to AI message; markdown when complete |
| Tool start        | Tool line / detail box                       |
| Tool output delta | Stream shell stdout into detail              |
| Tool done         | Finalize status and body                     |
| Turn done         | Token/cost footer; apply history             |

## Agent modes

Modes: `build`, `plan`, `ask`, `brave`.

- **Per-session** (not in `settings.json`); new sessions default to **`build`**
- Switched with **Shift+Tab** / mode UI; persisted on the session store when available
- Input border and footer colors reflect mode

| Mode               | Design behavior                          |
| ------------------ | ---------------------------------------- |
| build / plan / ask | Same at first; diverge via prompts later |
| brave              | Skip approval for risky tools            |

### Plan collaboration mode (`elph-agent`)

Distinct from the TUI `plan` agent mode above. `AgentHarness` supports Codex-style **Plan collaboration mode**:

1. Host calls `enter_plan_mode()` → `CollaborationMode::Plan` persisted on the session tree.
2. Active tools filter to read-only exploration; mutating and multi-agent tools are blocked at policy and `before_tool_call`.
3. Plan-mode system prompt is appended on each turn snapshot.
4. When the assistant wraps a plan in `<proposed_plan>...</proposed_plan>`, the harness emits `PlanProposed` then `PlanConfirmationRequired`.
5. Host calls `resolve_plan_confirmation(choice)` — `StayInPlan`, `Implement`, or `ImplementFresh` — before the agent edits files or runs shell commands.

Elph TUI wiring for plan confirmation is deferred.

### Subagents (`elph-agent`)

Child agents managed by `AgentControl` on the harness (Codex thread style). Design:

- `spawn_agent` creates a **persistent child** (`SessionDirStorage` + mini `AgentHarness`).
- Shared `AgentRegistry` across parent and children; `agent_path` for nested identity.
- `max_depth = 3`, `max_concurrent = 4`; children may spawn further children when depth allows.
- Multi-agent tools: `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents`.
- Graph edges persisted in `metadata.db` (`agent_spawn_edges`).
- Tool results return status only (`Spawned subagent <id>`, `<id> is idle`): the child's final answer text is streamed to the host UI and stored in the child session, never returned in the parent's tool results. The parent verifies child work through repository state (re-read changed files, `git diff`, tests).

TUI shows `agent_id` + `agent_path` in subagent status lines.

### Extensions (WASM)

Pi-compatible extension bundles discovered from `CONFIG_DIR/extensions/` (default `~/.config/elph/extensions/`) and `<project>/.elph/extensions/`. Phase 1: slash commands via wasmtime Component Model. `/reload` refreshes registry. See [extensions.md](./extensions.md).

## Thinking levels

Levels: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`.

- **Shift+Tab** cycles in the TUI
- Mapped per model via `thinkingLevelMap` in provider config
- Sent as token budget (Anthropic) or `reasoning_effort` (OpenAI-compatible)

## Sessions & logging

### Session ID

TypeID with prefix `sess` — shown in the footer.

### Persistence

| Data                 | Location                                                                           |
| -------------------- | ---------------------------------------------------------------------------------- |
| Provider / model     | Per-session (tree + Turso row); new sessions seed from the project's last used model, falling back to `models.defaultModel` (TUI) |
| Mode / thinking      | Per-session (default mode `build`; thinking seed `models.defaultThinkingLevel`)    |
| Conversation history | Turso session tree in `APP_DATA/metadata.db` (`session_entries`)                   |
| Platform metadata    | Same DB: goals, spawn graph, session index                                         |
| Model catalog        | Embedded + merge `CONFIG_DIR/providers/*.json` (disk wins by id)                   |
| Crash recovery       | Semi-durable harness journal + tool-result repair (see below)                      |
| Project memory       | `<project>/.elph/store.db`                                                         |
| Session artifacts    | `APP_DATA/sessions/<SESSION_ID>/` (`mcp_cache` tool result cache, `terminals`, `tool_outputs.jsonl`) |
| Todo snapshot        | Per-session metadata when TodoList is active                                       |
| Event / request logs | JSONL per session for diagnostics                                                  |

### Semi-durable harness recovery

Product open/resume uses `AgentHarness::restore` (wired from `elph/src/agent/runtime.rs`). Session open also runs `reconcile_session` in `SessionManager`.

**Journal** — custom tree entries with type prefix `harness.*`:

| Custom type                                                | Role                                                             |
| ---------------------------------------------------------- | ---------------------------------------------------------------- |
| `harness.queue_enqueue` / `harness.queue_consume`          | Durable steer / follow-up / next-turn queues (stable `queue_id`) |
| `harness.pending_write` / `harness.pending_write_applied`  | Deferred session writes while a turn runs (stable `write_id`)    |
| `harness.operation_started` / `harness.operation_finished` | Run / compaction / branch-summary lifecycle                      |
| `harness.turn_started` / `harness.turn_finished`           | Per-turn markers (including fail / interrupt outcomes)           |

**On restore:**

1. Repair unanswered `tool_use` with synthetic error tool results.
2. Close open operations as `interrupted`.
3. Rehydrate model / thinking / active tools / collaboration mode from the session tree.
4. Rehydrate in-memory queues and pending writes from the journal (`reduce_durable_state`).
5. Flush remaining pending writes when the harness is idle.

**Policies** (`RestoreOptions`): `MissingActiveToolsPolicy` (`DropMissing` default / `Fail`), `RecoveryPolicy` (`MarkInterrupted` default; `RetryUnfinished` reserved).

**Not journaled:** tool implementations themselves (host re-registers on open). Library hosts using `AgentHarness::new` alone skip rehydrate unless they call `restore` or `apply_durable_state`.

### Vision images (TUI)

- **Ctrl+V** / **Cmd+V** — paste up to 4 images when the model supports vision
- Stored under data dir `attachments/`
- Non-vision models: paths appended to text so the agent can use ReadMediaFile

## Goals & todos

### Goals (implemented)

Session objective with Codex-style lifecycle and budgets:

| Status                      | Meaning                                                       |
| --------------------------- | ------------------------------------------------------------- |
| `active`                    | Turn accounting runs; continuation steering when still active |
| `complete` / `blocked`      | Terminal; set via `update_goal` or `/goal`                    |
| `paused` / `budget_limited` | Blocks turns until resume or budget extend                    |

Tools: `create_goal`, `get_goal`, `update_goal`, `set_goal_budget`. Slash: `/goal` (status, pause, resume, cancel, replace, create). `/goal next` — **planned** (queued goals).

Turn hooks: harness `start_turn` / `finish_turn` with token/wall-clock accounting.

### TodoList (planned)

Tasks panel above input; per-session snapshot persistence.

## Built-in tool wiring (`elph`)

`create_coding_session_with_events` in `elph/src/agent/runtime.rs` assembles the harness tool list:

1. `BuiltinToolsBuilder::all(env)` — every built-in tool enabled by `elph-agent`’s `builtin-tools` feature
2. MCP tools from `McpToolRegistry::create_agent_tools().await`
3. Goal tools from `create_goal_tools()`

Multi-agent tools are injected by `AgentHarness` when `tools-multi-agent` is enabled and the default active-tool set is used.

## Related

- [extensions.md](./extensions.md) — WASM extension design
- [codebase-layout.md](./codebase-layout.md) — `elph` crate modules
- [tools.md](./tools.md) — catalog and approval
- [configuration.md](./configuration.md) — settings and paths
- [tui.md](./tui.md) — layout and keybindings
- [openwiki/architecture.md](../openwiki/architecture.md) — current implementation
