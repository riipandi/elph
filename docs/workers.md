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

## Worker names (memorable-ids)

Default display name is a **memorable-id** (same family as session titles, e.g. `calm-fox`). Override with `workers.name`. Collisions among live peers get a numeric suffix (`calm-fox2`). Target peers in `worker_send` / `worker_ask` by this name or by session id.

## Session lease

Opening a session acquires an exclusive lease. A second process that opens the **same** session fails with a clear conflict until the holder exits or the lease is reclaimed (heartbeat stale **and** holder pid dead).

## Exit / crash presence

| Event | What peers see |
| --- | --- |
| Clean exit / Drop | Worker marked `offline` immediately; file + session leases released |
| Crash / kill -9 | Next reaper tick (~1–2s) demotes when **pid is dead** or heartbeat exceeds `leaseStaleSecs` |
| Live list / TUI badge | `list_live` / heartbeat reaper demotes first — departed workers leave the live set |

This is **DB-poll near-realtime**, not a separate intercom socket.

## File leases (shared cwd)

Mutate tools (`edit_file`, `write_file`, `delete_path`, `move_path`, `copy_path` dest, `create_dir`) claim the path in `file_leases` first. If another live worker holds the path, the tool errors with the holder identity — no silent overwrite.

Claims last until process exit (heartbeat refreshed). Release on clean shutdown / Drop.

## Inter-worker messaging (pi-intercom-like)

Yes — peers communicate through the **durable mailbox** in `.elph/store.db` (not Unix sockets):

| Tool | Role |
| --- | --- |
| `worker_list` | Live peers (memorable names + status) |
| `worker_send` | Fire-and-forget to a peer by **name** or session id |
| `worker_ask` / `worker_get` / `worker_await` | Request + poll status |

Inbound mail is polled, injected as `worker.inbound` (idempotent by `msg_id`), then steered into the agent. **Reply in assistant text** — do not `worker_send` to answer an inbound ask.

Compared to classic intercom: delivery is **poll-based durable SoT** (survives restart); latency ≈ `inboxPollMs` / reaper interval, not sub-ms IPC.

## TUI

When **≥ 2** live workers and `tuiShowPeers` is true, the status footer shows a compact badge: `⬡ N`. Hidden when alone.

## Retention / GC

Session GC never deletes sessions that currently hold a row in `session_leases`.

## Limitations (v1)

- Same machine / shared store only
- No automatic merge of concurrent edits
- No cross-worktree file leases (by design)
- Ask auto-complete from assistant text on turn end is partial (host injects + steers; full ask-pair completion still uses tools/`worker_get`)
