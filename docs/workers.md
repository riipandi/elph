# Multi-worker coordination

Elph can run **multiple processes on the same project** as peer workers. Each process owns one session tree exclusively and coordinates through the shared project store (`.elph/store.db`).

## Model

| Concept | Meaning |
| --- | --- |
| **Worker** | One OS process ≈ one coding session |
| **Subagent** | In-process child of a session (not a peer worker) |
| **SoT** | SQLite tables only — notify / TUI never authority |

## What is durable

| State | Table |
| --- | --- |
| Session exclusivity | `session_leases` |
| Live presence | `workers` |
| Inter-worker mail | `worker_messages` |
| Path write claims | `file_leases` |

Schema version **202** (`elph_workers_v1`).

## Settings (`workers`)

```json
{
  "workers": {
    "enabled": true,
    "name": null,
    "purpose": "",
    "heartbeatSecs": 10,
    "leaseStaleSecs": 30,
    "inboxPollMs": 750,
    "askTimeoutMs": 600000,
    "maxHops": 5,
    "tuiShowPeers": true,
    "fileLeases": true
  }
}
```

- **Default on.** Alone in a project you see no TUI badge (count ≤ 1).
- Set `"enabled": false` to skip lease, registry, worker tools, and path claims.
- Prefer **git worktrees** for heavy parallel implementation (separate cwd → separate `project_key` → no file-lease collisions).

## Session lease

Opening a session acquires an exclusive lease. A second process that opens the **same** session fails with a clear conflict until the holder exits or the lease is reclaimed (heartbeat stale **and** holder pid dead).

## File leases (shared cwd)

Mutate tools (`edit_file`, `write_file`, `delete_path`, `move_path`, `copy_path` dest, `create_dir`) claim the path in `file_leases` first. If another live worker holds the path, the tool errors with the holder identity — no silent overwrite.

Claims last until process exit (heartbeat refreshed). Release on clean shutdown / Drop.

## Agent tools

| Tool | Role |
| --- | --- |
| `worker_list` | Live peers in this project |
| `worker_send` | Fire-and-forget message (not for replies) |
| `worker_ask` / `worker_get` / `worker_await` | Request + poll status |

Inbound mail is polled from the durable mailbox, injected as a `worker.inbound` custom entry (idempotent by `msg_id`), then steered into the agent as a normal turn. **Reply in assistant text** — do not `worker_send` to answer an inbound ask.

## TUI

When **≥ 2** live workers and `tuiShowPeers` is true, the status footer shows a compact badge: `⬡ N`. Hidden when alone.

## Retention / GC

Session GC never deletes sessions that currently hold a row in `session_leases`.

## Limitations (v1)

- Same machine / shared store only
- No automatic merge of concurrent edits
- No cross-worktree file leases (by design)
- Ask auto-complete from assistant text on turn end is partial (host injects + steers; full ask-pair completion still uses tools/`worker_get`)
