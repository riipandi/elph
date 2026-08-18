# Multi-worker coordination

Elph can run **multiple processes on the same project** as peer workers. Each process owns one session tree exclusively and coordinates through the shared project store (`.elph/store.db`).

## Model

| Concept | Meaning |
| --- | --- |
| **Worker** | One OS process ≈ one coding session (parallel instance of the same agent working on the same project) |
| **Subagent** | In-process child of a session (separate delegated AI agent with its own context window for independent tasks - different projects, different domains) |
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

Opening a session acquires an exclusive lease. A second process that opens the **same** session fails with a clear conflict while the holder process is still alive.

**Reclaim after crash / force-quit:** if the holder **PID is dead**, the next open reclaims the lease **immediately** (no wait for `leaseStaleSecs`). Previously reclaim required both a stale heartbeat **and** a dead PID, so restarting Elph within ~30s after a crash failed with “session is leased by worker …”.

If the holder PID is still alive, you must close that Elph instance or open a different session.

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

Peers communicate through the **durable mailbox** in `.elph/store.db` (not Unix sockets):

| Tool | Role |
| --- | --- |
| `worker_list` | Live peers (memorable names + status) |
| `worker_send` | Fire-and-forget to a peer by **name** or session id |
| `worker_ask` / `worker_get` / `worker_await` | Request + poll status |
| `worker_reply` | Threaded reply to an inbound message (keeps the conversation thread). `in_reply_to` optional — omitted replies the single pending inbound ask |
| `worker_pending` | List inbound asks still waiting for an answer (with their `msg_id`) |

### Worker chat (Alt+M / `/intercom`)

Worker messaging is **not** a side question like `/aside` — it is a first-class
threaded chat with its own mechanism:

- **Alt+M** (or `/intercom`) opens the **worker chat overlay**: a picker of live
  peers, then per-peer thread history, plus a compose field. Enter sends; Esc
  goes back to the picker, then closes.
- Messages are **threaded** (`conversation_id`): `worker_reply` from the agent,
  and the TUI compose box, continue the same thread instead of starting a new
  one.
- Sending from the TUI never routes through the agent turn — the message goes
  straight to the peer's mailbox, so it never interrupts your current task.

### Inbound messages are answered in parallel

Inbound mail is polled (`inboxPollMs`), then delivered:

- The message lands in the worker chat inbox and the TUI shows an unread badge.
- **Only new messages** (no `parent_msg_id`) trigger an answer. The poller
  **spawns** a snapshot completion (worker tools only: `worker_reply`,
  `worker_list`, `worker_pending`) with an intercom wrapper; it does not wait
  for the user's harness turn.
- The answer runs on a **parallel intercom loop** (conversation snapshot +
  worker tools only). It uses **`intercom_base`** — not `coding_base` — so the
  model is not instructed to edit, shell, or plan a user task. It does **not**
  wait on the turn gate or call `harness.prompt`, so a busy worker can
  `worker_reply` while the user's task continues. The peer's `worker_ask`
  unblocks as soon as the mailbox response is written.
- If the answer turn fails, the ask is closed with an explicit error reply
  (`kind = response`) so the peer's `worker_get` / `worker_await` unblocks.
- **Threaded replies** (`worker_reply` / TUI chat answers — `parent_msg_id` set)
  are delivered to the inbox **only**: they resolve the asker's pending ask via
  the mailbox and never spawn another turn. This loop guard prevents two idle
  workers from replying to each other forever.

Inbound messages **never steer or interrupt** the user's current agent turn —
the answer loop never takes `turn_gate`, never calls the harness, and never
sets a flag that would suppress user-turn transcript events. A TUI “replying”
badge (`intercom_replying`) is chrome only. The intercom loop is **not
appended to the user transcript** (same as `/aside`): the worker chat overlay
is the only surface for that dialogue. Shutdown aborts in-flight intercom
tasks; if the mailbox already has a reply, the loop does not write a second
one.

Compared to classic intercom: delivery is **poll-based durable SoT** (survives
restart); latency ≈ `inboxPollMs` / reaper interval, not sub-ms IPC.

### Local notify (v2 — not implemented)

A future optional **wake channel** (Unix socket / FS watch) may reduce poll latency for inbox and presence. It must stay **best-effort only** — mailbox + `workers` / `session_leases` / `file_leases` remain the source of truth. Missed notifies recover via poll.

## TUI

When **≥ 2** live workers and `tuiShowPeers` is true, the status footer shows a compact badge: `⬡ N`. Hidden when alone.

## Retention / GC

Session GC never deletes sessions that currently hold a row in `session_leases`.

## Limitations (v1)

- Same machine / shared store only
- No automatic merge of concurrent edits
- No cross-worktree file leases (by design)
- No sub-second IPC wake channel — delivery/presence is poll-based (see the
  notify design above). A busy worker answers inbound asks **in parallel**
  with its current user task (snapshot + worker tools; no write/shell tools).
