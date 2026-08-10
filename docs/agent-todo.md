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

| Field     | Notes                                                                          |
| --------- | ------------------------------------------------------------------------------ |
| `merge`   | `true` (default): upsert by `id`. `false`: replace entire list                 |
| `todos`   | Array; empty + `merge: false` clears the list                                  |
| `id`      | Optional on create; required to update. Host mints `todo_<kalid>` when omitted |
| `content` | Actionable title; optional on merge when only changing status                  |
| `status`  | `pending` \| `in_progress` \| `completed` \| `cancelled`                       |

Rules enforced by the store:

- At most one `in_progress` per write (merge demotes extras if needed)
- Duplicate ids in one call are rejected
- Minted ids are checked against the current list so a same-row `replace`
  (delete-all + reinsert) can never hit a PRIMARY KEY collision; the
  inserted ids are always unique

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

## TUI

Live panel above the status row, driven by `TodoUpdated` UI events from `todo_write`:

| Behavior | Detail |
| -------- | ------ |
| Chrome | Minimal round border; title inline on the top edge as `(Todos done/total)` |
| Rows | Unfinished only (`○` pending / spinner when `in_progress`); finished hidden |
| Hide | Entire panel disappears when every item is `completed` or `cancelled` (or the list is empty) |
| Cap | At most 5 open rows; extra shown as `↓N more` |
| Steer / interject | Queued steer (Ctrl+Enter) or activity `Steering` dims rows, pauses spinner emphasis, and annotates the title as `(Todos x/x · steered)` so the checklist reads as provisional until the agent rewrites it |

Tool results still return the full list JSON for the model; the host mirrors that into the panel.
