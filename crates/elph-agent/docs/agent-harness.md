# AgentHarness lifecycle

`AgentHarness` is the orchestration layer above the low-level agent loop. It owns session persistence, runtime configuration, resource resolution, operation locking, and extension-facing mutation semantics.

This document describes the current direction and implemented behavior in `elph-agent`. Some extension/session-facade details are planned and called out explicitly.

## Ultimate lifecycle goal

Harness listeners and hooks should be able to close over the `AgentHarness` instance and call public harness APIs from any event where those APIs are documented as allowed. Those calls must not corrupt in-flight turn snapshots, reorder persisted transcript entries, lose pending writes, deadlock settlement, or leave the harness in the wrong phase.

The intended rule is:

- structural operations remain rejected while busy
- queue operations are accepted at documented turn-safe points
- runtime config setters update future snapshots without mutating the current provider request
- session writes made while busy are durably queued and flushed in deterministic order
- getters return latest harness config, not in-flight snapshots
- listeners/hooks that call settlement APIs such as `wait_for_idle()` during the active run can deadlock; prefer `abort()` or queue operations from turn-safe points

Provider transport streaming is decoupled from downstream event consumption. The harness can therefore await listeners, extension hooks, persistence, and save-point work without blocking the provider transport reader.

## Error handling

The current split is:

- low-level capabilities and helpers use `HarnessResult<T, E>` (aliased as `Result<T, E>` in the harness module) where expected failures are contained and must not throw, such as `ExecutionEnv`, filesystem/shell operations, shell-output capture, resource loading, and compaction helpers
- high-level mutation/orchestration APIs such as `Session` and `AgentHarness` return `HarnessOpResult<T>` (`Result<T, AgentHarnessError>`) instead of bare results that can be ignored
- public `AgentHarness` failures are normalized to `AgentHarnessError` where practical; subsystem errors are preserved as `cause`

Harness events observe committed state. Public mutators validate required input and persistence before committing when practical, then await notifications. If a hook or subscriber fails after commit, the state change is not rolled back and the public method returns `AgentHarnessError` with code `Hook`.

## State model

The harness separates state into four categories.

### Harness config

Harness config is the latest runtime configuration set by the application or extensions:

- model
- thinking level
- tools
- active tool names
- resources
- stream options
- system prompt or system prompt provider

Getters (`get_model`, `get_thinking_level`, `get_tools`, `get_active_tools`, `get_resources`, `get_stream_options`) return harness config. They do not return the snapshot used by an in-flight provider request.

Setters (`set_model`, `set_thinking_level`, `set_tools`, `set_active_tools`, `set_resources`, `set_stream_options`) update harness config immediately, including while a turn is in flight. Changes affect the next turn snapshot, not the currently running provider request.

`set_resources()` accepts concrete resources and emits `resources_update` on every call with shallow-copied current and previous resources. Applications own loading/reloading resources from disk or other sources and should call `set_resources()` with new values.

### Turn snapshot

A turn snapshot is the concrete state used for one LLM turn. It is created by `create_turn_state()` and contains:

- persisted session messages
- resolved resources
- resolved system prompt
- model
- thinking level
- all tools
- active tools
- stream options
- derived session id

Static option values are used directly. System-prompt provider callbacks are invoked once per `create_turn_state()` call. All logic for that turn uses the same snapshot.

Resource arrays are shallow-copied when a snapshot is created. Individual skill and prompt-template objects are not deep-copied.

Stream options are shallow-copied when a snapshot is created. `headers` and `metadata` maps are shallow-copied; their values are not deep-copied. Credentials from `get_api_key` resolve per provider request so expiring tokens can refresh, but the configured stream options and derived session id come from the current turn snapshot.

### Session

The session contains persisted entries only. Session reads return persisted state and do not include queued writes.

`session_entries()` returns the current branch entries from persisted session state.

Session storage implementations must persist leaf changes as `leaf` entries. `set_leaf_id()` is not an in-memory-only cursor update; it appends a durable entry whose `target_id` is the active tree leaf or `null` for root. Reopening storage must reconstruct the current leaf from the latest persisted leaf-affecting entry.

Backends:

- `InMemorySessionStorage` — tests and ephemeral use
- `JsonlSessionStorage` — append-only file sessions
- `TursoSessionStorage` — SQL-backed durable sessions

### Pending session writes

Session writes requested while an operation is active are queued as pending session writes. Pending writes are based on session-entry shapes without generated fields (`id`, `parent_id`, `timestamp`).

Pending session writes are always persisted. They are flushed at save points, at operation settlement, and in failure cleanup.

## Operation phases

The harness has an explicit phase:

```rust
pub enum AgentHarnessPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}
```

Structural operations require `phase == Idle` and set the phase before the first `await`:

- `prompt`
- `skill`
- `prompt_from_template`
- `compact`
- `navigate_tree`

Starting another structural operation while the harness is not idle returns `AgentHarnessError` with code `Busy`.

The following operations are allowed during a turn where appropriate:

- `steer`
- `follow_up`
- `next_turn`
- `abort`
- runtime config setters

## Turn execution

`prompt`, `skill`, and `prompt_from_template` follow the same flow:

1. Assert idle and set phase to `Turn`.
2. Create a turn snapshot with `create_turn_state()`.
3. Derive invocation text from that snapshot.
4. Execute the turn with `execute_turn()`.

`skill` and `prompt_from_template` resolve their resource from the same snapshot that is passed to the turn. They do not resolve resources separately.

`steer`, `follow_up`, and `next_turn` accept text plus optional images and create user messages internally. `next_turn` messages are inserted before the new user message on the next user-initiated turn.

Queue modes are live, not turn-snapshotted:

- `get_steering_mode()` / `set_steering_mode()`
- `get_follow_up_mode()` / `set_follow_up_mode()`

Changing a queue mode during a run affects the next queue drain. Queue drains happen at safe points.

## Save points

A save point occurs after an assistant turn and its tool-result messages have completed.

At a save point the harness:

1. flushes pending session writes after the agent-emitted messages for that turn
2. creates a fresh turn snapshot if the low-level loop may continue
3. applies the fresh context/model/thinking-level/stream-options/session-id state before the next provider request

This lets model, thinking level, tool, resource, stream option, and system prompt changes made during a turn affect the next turn in the same run, while never mutating an in-flight provider request.

The low-level loop converts harness `AgentThinkingLevel` to provider `reasoning` at the provider boundary:

- `Off` → `None`
- all other thinking levels pass through

No state refresh is needed on `agent_end` except flushing leftover pending session writes and clearing the operation phase.

If the system-prompt callback returns an error while starting `prompt`, `skill`, or `prompt_from_template`, the operation fails and the harness returns to idle. If it fails from the save-point snapshot created by `prepare_next_turn`, the low-level agent run records an assistant error message.

## Hooks and events

The hook system is described in [hooks.md](./hooks.md).

Summary:

- `AgentHarness` emits typed hook events and consumes typed results.
- `HookRegistry` owns registration and result reducers.
- Typed handlers: `on_before_agent_start`, `on_context`, `on_tool_call`, `on_tool_result`
- Observational handlers: `subscribe` (all events)
- `subscribe` listeners are cloned before invocation to avoid deadlocks when listeners call back into the harness.

Agent-emitted messages are persisted on `message_end` to preserve transcript ordering. Pending extension/session writes flush after those messages at save points.

## Abort

Abort is allowed during a turn. It aborts the low-level run and clears steering/follow-up queues.

```rust
harness.abort().await?;
```

Abort does not clear `next_turn` messages. Messages queued with `next_turn()` survive abort and are inserted before the user message on the next user-initiated turn.

Abort does not discard pending session writes. Pending writes flush at the next save point if reached, at `agent_end`, or in operation failure cleanup.

## Compaction and tree navigation

Compaction and tree navigation are structural session mutations.

They are allowed only while idle and are not queued. They operate on persisted session state. The next prompt creates a fresh turn snapshot.

```rust
harness.compact(None).await?;
harness.navigate_tree(target_entry_id, None).await?;
```

Branch summary generation is part of the tree navigation operation.

Auto-compaction and retry decision points are not implemented in `AgentHarness` yet.

## Execution environment

`LocalExecutionEnv` implements `ExecutionEnv` for filesystem and shell operations used by built-in tools, skill/template loaders, and compaction helpers:

```rust
use elph_agent::LocalExecutionEnv;
use std::sync::Arc;

let env = Arc::new(LocalExecutionEnv::new(cwd));
```

`ExecutionEnv` methods return `HarnessResult<T, ExecutionError>` or `HarnessResult<T, FileError>` — expected failures do not panic.

Built-in tool wiring:

- `read`, `write`, `edit`, `bash`, `ls` — `ExecutionEnv` file and shell APIs
- `grep`, `find` — resolve paths via `ExecutionEnv`, then search the real filesystem with [`fff-search`](https://crates.io/crates/fff-search) (`FilePicker::collect_files`, `watch: false`)

See [tools.md](./tools.md) for tool groups, parameters, and output formats.

## Test organization

Harness tests live in `crates/elph-agent/tests/harness.rs` and cover:

- queue modes (`OneAtATime` / `All`)
- steering and follow-up drain
- `before_agent_start` hook injection
- `tool_result` hook patching
- context hook failure propagation

Use the `elph-ai` faux provider (`faux_provider`, `faux_assistant_message`) for deterministic harness/provider tests. Faux response factories can inspect stream options and return scripted assistant messages without real provider APIs or network access.

Run harness tests:

```bash
cargo test -p elph-agent --test harness
```

## Implementation status

### Done

- `AgentHarness` calls `run_agent_loop()` directly
- Harness owns run lifecycle, abort controller, queue draining, provider stream config, event reduction, session persistence, pending write flushing, and save-point snapshots
- Explicit `phase` in place of boolean idle state
- Save points refresh context, model, thinking level, stream options, and session snapshot state
- Pending session writes flush at save points, settlement, and failure cleanup
- `steer`, `follow_up`, and `next_turn` create user messages from text plus optional images
- Provider hooks: `before_provider_request`, `before_provider_payload`, `after_provider_response`
- Typed hook handlers with result chaining
- `ExecutionEnv` with typed `Result` returns
- Built-in coding and exploration tools (`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`)
- `grep` / `find` backed by `fff-search`; other tools use `ExecutionEnv` directly

### Planned

- Session facade for extension writes with pending-write ordering
- Auto-compaction decision point in harness
- Retry handling
- Semi-durable harness recovery (see [durable-harness.md](./durable-harness.md))
- Broad listener/hook reentrancy test suite
- Observability instrumentation (see [observability.md](./observability.md))
