# Agent todos

Session-scoped structured work lists so the agent plans multi-step work without repeating completed steps.

## Tools

Registered on coding sessions alongside goals:

| Tool         | Role                                              |
| ------------ | ------------------------------------------------- |
| `todo_write` | Create/update the list (`merge` default **true**) |
| `todo_read`  | List current todos without changing them          |

### `todo_write` input

```json
{
    "merge": true,
    "todos": [{ "id": "todo_…", "content": "…", "status": "pending" }]
}
```

| Field     | Notes                                                                                                                                                                                                                                                                                                                     |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `merge`   | `true` (default): upsert by `id`. `false`: replace entire list                                                                                                                                                                                                                                                            |
| `todos`   | Array; empty + `merge: false` clears the list                                                                                                                                                                                                                                                                             |
| `id`      | Optional on create; required to update. Host mints `todo_<kalid>` when omitted. Short labels (`"1"`, `"step_a"`) are accepted and mapped to a **session-scoped** PK so they never collide with other sessions (global `session_todos.id` PRIMARY KEY). Prefer returning/using the ids from tool results for later merges. |
| `content` | Actionable title; optional on merge when only changing status                                                                                                                                                                                                                                                             |
| `status`  | `pending` \| `in_progress` \| `completed` \| `cancelled`                                                                                                                                                                                                                                                                  |

Rules enforced by the store:

- At most one `in_progress` per write (merge demotes extras if needed)
- Duplicate ids in one call are rejected (after session-scoping / mint)
- Minted ids are checked against the insert batch so a same-row `replace`
  (delete-all + reinsert) can never hit a PRIMARY KEY collision
- Agent short ids are rewritten to `td_<session12>_<slug>` (deterministic per
  session) so `--continue` / multi-session use cannot hit
  `UNIQUE constraint failed: session_todos.id`

### Honest progress

`todo_write` rejects `completed` transitions that cannot prove real work:

- Marking an item `in_progress` snapshots the mutating-work counter; completing
  it requires the counter to advance after that snapshot.
- Items that enter the plan without `in_progress` get a creation baseline, so a
  model that skips the `in_progress` step can still complete items — as long as
  work happened after they were created. Create-and-complete in one call with no
  prior work is still rejected.
- A non-empty `reason` on a `completed` transition bypasses the check (analysis,
  review, MCP-driven work).

### Auto-close after a turn

Safety net for models that finish work but never update statuses. After every
turn whose final answer signals completion — a text-only final message carrying
a completion marker, no continuation marker (`not done`, `continuing`, …), on a
non-error turn — the harness closes the still-open todos it
can prove (work recorded since the item entered the plan, or mutating tool calls
during the turn). Tool-call cycles, errored/aborted turns, and mid-work answers
never trigger auto-close. Closed items emit `TodoUpdated` like a normal
`todo_write`.

## Goals vs todos

|              | Goals                                                       | Todos                           |
| ------------ | ----------------------------------------------------------- | ------------------------------- |
| Scope        | Session objective + budgets                                 | Step checklist for current work |
| Tools        | `create_goal`, `get_goal`, `update_goal`, `set_goal_budget` | `todo_write`, `todo_read`       |
| Blocks turns | budget / pause states                                       | Never                           |

## ReAct usage

System prompt (`coding_base.md` → `<operating_loop>`) biases to **See → Do → Check**, not ceremony:

1. **See** — conversation + already-injected memory/codegraph/tool results
2. **Do** — smallest tool set that advances the request
3. **Check** — validation that covers your change only

**Todos are rare, not ritual:** use `todo_write` only for true multi-step work (~4+ independent steps). Most tasks need zero todos. At most one `in_progress`; prefer status merges.

**Anti-overthinking:** no long pre-read tours, no speculative parallel searches, no memory/tool rituals before every edit, no recaps of tool output.

## Persistence

Table `session_todos` in `.elph/store.db`, cascade-deleted with the session. See [session-persistence.md](./session-persistence.md).

## Session restore

On session open (`--continue` / `--resume` / mid-session), open todos are:

1. Loaded from `session_todos` and emitted as `TodoUpdated` so the TUI panel rehydrates immediately.
2. Injected into the system prompt as part of `<session_state>` (with goal + last-message anchors) so the model continues the checklist instead of inventing a new plan.

## TUI

Live panel above the status row, driven by `TodoUpdated` UI events from `todo_write` and by rehydrate-on-open:

| Behavior          | Detail                                                                                                                                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chrome            | Minimal round border; title inline on the top edge as `Todos done/total`                                                                                                                                |
| Rows              | Unfinished only (`○` pending / spinner when `in_progress`); finished hidden                                                                                                                             |
| Hide              | Entire panel disappears when every item is `completed` or `cancelled` (or the list is empty)                                                                                                            |
| Cap               | At most 5 open rows; extra shown as `↓N more`                                                                                                                                                           |
| Steer / interject | Queued steer (Ctrl+Enter) or activity `Steering` dims rows, pauses spinner emphasis, and annotates the title as `Todos x/x · steered` so the checklist reads as provisional until the agent rewrites it |

Tool results still return the full list JSON for the model; the host mirrors that into the panel.
