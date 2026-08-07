# Subagent output durability and tracing

Every subagent in Elph writes a **persistent, traceable artifact set** under the
session data root, and its **final assistant text flows back to the parent**
instead of a blank status line.

## Motivation

Previously `wait_agent` / `followup_task` tool results carried only status
(`<agent_id> is idle`). Coupled with a wait/start race (the background turn task
could not have reached the harness run loop before `wait_for_idle()` resolved),
parents regularly ended up with `agent_xxx finished but returned no output` —
the model's own description of an empty tool result.

## Runtime behavior

### Tool results now carry output

| Tool            | Result content                                          |
| --------------- | ------------------------------------------------------- |
| `spawn_agent`   | `Spawned subagent <id>` (unchanged; background turn)     |
| `wait_agent`    | Final assistant text of the completed turn, or a readable placeholder (`<agent_id> is idle (no output captured)` / full-log path) |
| `followup_task` | `Turn started on <agent_id>` (unchanged; turn runs in background) |
| `list_agents`   | `SubagentInfo[]` including `model` (`provider/model_id`), `output: {text, output_path, turns, finished_at_ms}` |
| `send_message`  | `Message queued` (unchanged)                             |

`SubagentInfo.output` always has a non-empty `summary()`: the final text when
present, otherwise `(no text output — full log: <output.md path>)` or a
fallback placeholder — tool results never look empty.

### Persisted artifacts

When the host configures `SubagentBootstrap.outputs_root` (the coding-agent
passes `APP_DATA/sessions/<SESSION_ID>`), each spawned agent writes to

```
~/.local/share/elph/sessions/<SESSION_ID>/subagents/<agent_id>/
├── output.md      # final assistant text (rewritten each completed turn)
├── events.jsonl   # streamed text deltas (append-only, replayable)
└── meta.json      # spawn metadata (id, task_name, agent_path, depth, model `provider/model_id`, session ids)
```

Leading path components follow `ELPH_DATA_DIR` / `ELPH_HOME` when set. All
writes are best-effort (`std::fs` results are swallowed) — persistence never
blocks the agent loop or fails a spawn.

### Waiting is race-free

`SubagentHarness` tracks in-flight turns with a `(counter, Notify)` pair:

- `followup_task` calls `turn_started()` **synchronously** before dispatching
  the background task.
- The background task calls `turn_finished()` after the harness returns idle.
- `wait_agent` first awaits `wait_for_turn()`, then `wait_for_idle()` — so a
  caller can never observe "already idle" before the dispatched turn starts.

## Implementation notes

- `crates/elph-agent/src/agent/subagent/types.rs` — `SubagentOutput`,
  `SubagentBootstrap.outputs_root`, `persist` helpers (`output.md`,
  `events.jsonl`, `meta.json`).
- `crates/elph-agent/src/agent/subagent/harness.rs` — `SubagentHarness`
  captures the `AssistantMessage` returned by `AgentHarness::prompt`, persists
  it, and folds it into `output()`.
- `crates/elph-agent/src/agent/subagent/control.rs` — wait paths use
  `wait_for_turn()`; `wait_agent_cancellable_for_output` returns final text.
- `crates/elph-agent/src/tools/collaboration.rs` — `wait_agent` passes the
  output string straight to the tool result.
- `crates/coding-agent/src/agent/runtime.rs` — sets `outputs_root` to the
  session artifact dir.
- `crates/coding-agent/src/agent/session/wiring.rs` — the subagent event
  forwarder appends streamed deltas to `events.jsonl` via
  `elph_agent::subagent_persist_event`.
