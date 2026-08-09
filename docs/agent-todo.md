# Agent todos

Session-scoped structured work lists so the agent plans multi-step work without repeating completed steps.

## Tools

Registered on coding sessions alongside goals:

| Tool | Role |
| --- | --- |
| `todo_write` | Create/update the list (`merge` default **true**) |
| `todo_read` | List current todos without changing them |

### `todo_write` input

```json
{
  "merge": true,
  "todos": [
    { "id": "todo_…", "content": "…", "status": "pending" }
  ]
}
```

| Field | Notes |
| --- | --- |
| `merge` | `true` (default): upsert by `id`. `false`: replace entire list |
| `todos` | Array; empty + `merge: false` clears the list |
| `id` | Optional on create; required to update. Host mints `todo_<kalid>` when omitted |
| `content` | Actionable title; optional on merge when only changing status |
| `status` | `pending` \| `in_progress` \| `completed` \| `cancelled` |

Rules enforced by the store:

- At most one `in_progress` per write (merge demotes extras if needed)
- Duplicate ids in one call are rejected

## Goals vs todos

| | Goals | Todos |
| --- | --- | --- |
| Scope | Session objective + budgets | Step checklist for current work |
| Tools | `create_goal`, `get_goal`, `update_goal`, `set_goal_budget` | `todo_write`, `todo_read` |
| Blocks turns | budget / pause states | Never |

## ReAct usage

System prompt (`coding_base.md`) instructs:

1. **Observe** — conversation, memory, codegraph, todos  
2. **Plan** — for ~3+ step work, write todos early; one `in_progress`  
3. **Act** — tools for the current step only  
4. **Evaluate** — mark completed/cancelled before the next step  

Skip todos for trivial single-step asks. Prefer merge status updates over full rewrites.

## Persistence

Table `session_todos` in `.elph/store.db`, cascade-deleted with the session. See [session-persistence.md](./session-persistence.md).

## TUI

Tasks panel (pending / in progress above the prompt) is planned to bind to this store; tool results already return the full list JSON for the model and host to refresh UI.
