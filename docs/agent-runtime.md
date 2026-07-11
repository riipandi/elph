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

- Default **25** rounds per turn (`maxToolIterations`; `0` = default).
- On limit: stop with a clear message to the user.

## System prompt

Assembly order:

1. System template + active tool list
2. Project context — nearest `AGENTS.md`
3. Registered skills — metadata in prompt; body read by agent when relevant
4. Current date, working directory, session mode
5. Guardrails, thinking instructions, response language (`preferedResponseLanguage`)

## Tool loop

1. Send **exposed** tool schemas to the provider.
2. Receive `tool_calls` / `tool_use` from the stream.
3. Interactive tools: block until the user answers.
4. Risky tools: approval dialog (unless brave / allow-for-session).
5. Execute; stream shell output to the TUI when applicable.
6. Optionally rewrite structured tool output as [TOON](https://github.com/toon-format/toon) before the model sees it (`ELPH_PROMPT_ENCODING` or harness config; default `off`).
7. Append assistant + tool result messages to history.
8. Repeat until no tool calls remain.

### TOON prompt encoding (optional)

When enabled, the agent runtime may compress large JSON tool results (and MCP `structured_content`) into TOON fenced blocks in model-visible `content`. This reduces input tokens on tabular payloads; wire/API JSON is unchanged.

| Mode   | Behavior                                              |
| ------ | ----------------------------------------------------- |
| `off`  | Default — tool results pass through unchanged         |
| `toon` | Encode eligible JSON ≥ size threshold                 |
| `auto` | Encode only uniform tabular JSON arrays               |

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

### Auto-compaction on context overflow

When the provider returns context-too-large and `autoCompactContext` is true:

- Up to 3 retries with increasing aggressiveness (2×, 4×, 8× default limits)
- Floor: 4 messages / 16 KB; 4 KB minimum tool-result truncation
- Target percentage: `autoCompactLimit` (default 80%)

Manual: `/compact [pct]` (alias `/c`).

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

- Stored in settings → `session.agentMode`
- Switched with **Ctrl+A** or footer click
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

Elph TUI wiring for plan confirmation is deferred; Owly is out of scope for this flow.

### Subagents (`elph-agent`)

Child agents managed by `AgentControl` on the harness (Codex thread style). Design:

- `spawn_agent` creates a **persistent child** (`SessionDirStorage` + mini `AgentHarness`).
- Shared `AgentRegistry` across parent and children; `agent_path` for nested identity.
- `max_depth = 3`; children may spawn further children when depth allows.
- Multi-agent tools: `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents`.
- Graph edges persisted in `metadata.db` (`agent_spawn_edges`).

TUI shows `agent_id` + `agent_path` in subagent status lines.

### Extensions (WASM)

Pi-compatible extension bundles discovered from `~/.elph/extensions/` and `<project>/.elph/extensions/`. Phase 1: slash commands via wasmtime Component Model. `/reload` refreshes registry. See [extensions.md](./extensions.md).

## Thinking levels

Levels: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`.

- **Shift+Tab** cycles in the TUI
- Mapped per model via `thinkingLevelMap` in provider config
- Sent as token budget (Anthropic) or `reasoning_effort` (OpenAI-compatible)

## Sessions & logging

### Session ID

TypeID with prefix `sess` — shown in the footer.

### Persistence

| Data                 | Location                                     |
| -------------------- | -------------------------------------------- |
| Provider / model     | `~/.elph/settings.json` (home wins)          |
| Mode / thinking      | Merged home + project settings               |
| Conversation history | In-memory + durable backend                  |
| Platform metadata    | `metadata.db` in data dir                    |
| Project memory       | `<project>/.elph/store.db`                   |
| Todo snapshot        | Per-session metadata when TodoList is active |
| Event / request logs | JSONL per session for diagnostics            |

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

## Related

- [extensions.md](./extensions.md) — WASM extension design
- [codebase-layout.md](./codebase-layout.md) — `elph` crate modules
- [tools.md](./tools.md) — catalog and approval
- [configuration.md](./configuration.md) — settings and paths
- [tui.md](./tui.md) — layout and keybindings
- [openwiki/architecture.md](../openwiki/architecture.md) — current implementation
