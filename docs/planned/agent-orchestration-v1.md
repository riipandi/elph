# Plan (revisi): Multi-Worker Coordination — Durable, Solid, Resilient + TUI

## Goal

Enable **multiple Elph processes on one project** as peer workers (**1 process ≈ 1 agent/session**), with:

1. **Durable** mailbox and registry in project DB (survives crash/restart)
2. **Solid** exclusive session leases (no dual writers on one tree)
3. **Resilient** delivery (poll recovers; notify is never SoT; stale lease reclaim)
4. **TUI indicator** when **≥2 live workers** in the project; **hidden when only self**

v1 is **same machine / same project store**. Multi-host hub is out of scope.

---

## Durability / solid / resilient (non-negotiable)

### Durable (SoT survives process death)

| State                                           | Must persist in `.elph/store.db`                  |
| ----------------------------------------------- | ------------------------------------------------- |
| Worker identity (name, session_id, project_key) | `workers`                                         |
| Exclusive session ownership                     | `session_leases`                                  |
| Outbound/inbound mail + status machine          | `worker_messages`                                 |
| Recipient transcript after inject               | existing `session_entries` (same as normal turns) |

**Never treat as SoT:** Unix sockets, in-memory maps, TUI widget state, notify wakes.

### Solid (correct under concurrency)

| Rule                             | Mechanism                                                                     |
| -------------------------------- | ----------------------------------------------------------------------------- |
| One writer per session tree      | Exclusive `session_leases` + fail open if live foreign lease                  |
| Claim-once inbox                 | Optimistic `UPDATE … WHERE status='queued'` → `delivered` (rows_affected = 1) |
| Short transactions               | BEGIN IMMEDIATE only for claim/send/lease; no long holds during LLM stream    |
| FK + `PRAGMA foreign_keys=ON`    | Already on connections; new tables cascade with `sessions` where safe         |
| Name uniqueness among live peers | App-level: demote stale first, then assign suffix on collision                |

### Resilient (recover from failure)

| Failure                          | Recovery                                                                                                                                          |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Receiver offline                 | Mail stays `queued`; delivered on next poll after peer online                                                                                     |
| Receiver crash mid-turn          | Message already `delivered` + injected in tree; ask may timeout → `timeout`; no double-inject (idempotent by `msg_id` custom entry / claim table) |
| Sender crash waiting on ask      | On resume, `worker_get` can still poll message status from DB                                                                                     |
| Missed notify                    | Poll interval recovers all `queued`                                                                                                               |
| Stale lease (hard kill)          | Reclaim if `heartbeat_at` > `leaseStaleSecs` **and** pid dead (or force)                                                                          |
| Half-dead peer (pid alive, hung) | Heartbeat stop → status `stale` then `offline`; lease reclaim only after stale window                                                             |
| Hop / ping-pong                  | `maxHops` + system-prompt: never `worker_send` to reply to inbound                                                                                |
| GC vs active worker              | Retention treats **leased** sessions as protected                                                                                                 |
| Process exit                     | `Drop` / signal: best-effort release lease + mark offline (timeout bounded)                                                                       |

### Message state machine

```text
queued → delivered → complete
                  ↘ error
                  ↘ timeout
```

- **send:** insert `queued`
- **claim (receiver):** `queued` → `delivered` (atomic)
- **ask complete:** receiver posts `response` row **or** updates pair; asker sees `complete`
- **timeout:** asker or sweeper marks `timeout` after `askTimeoutMs`

### Inject idempotency

Store `worker_messages.id` on injected session custom entry details (`worker.inbound`, `msg_id=…`). Before inject, skip if branch already contains that `msg_id`. Prevents double-trigger after crash between claim and inject commit.

---

## Current baseline

| Capability                        | Status      |
| --------------------------------- | ----------- |
| Shared project DB (WAL + FK v201) | Done        |
| Many sessions per cwd             | Done        |
| Semi-durable single session       | Done        |
| In-process subagents              | Done        |
| Lease / registry / mailbox        | **Missing** |
| Multi-worker TUI                  | **Missing** |

---

## Mental model

```text
 Terminal A: elph                 Terminal B: elph
 session S_a + exclusive lease    session S_b + exclusive lease
 worker name "planner"            worker name "worker"
        │                                │
        └──────────────┬─────────────────┘
                       ▼
                .elph/store.db
         sessions · entries · turns
         session_leases · workers · worker_messages
```

**Subagent** = in-process, same UI tree.  
**Worker** = separate OS process + own session + mailbox.

---

## Schema (migration **v202**, additive only)

### `session_leases`

```sql
CREATE TABLE session_leases (
  session_id   TEXT PRIMARY KEY NOT NULL
               REFERENCES sessions(id) ON DELETE CASCADE,
  worker_id    TEXT NOT NULL,
  pid          INTEGER NOT NULL,
  hostname     TEXT,
  acquired_at  TEXT NOT NULL,
  heartbeat_at TEXT NOT NULL,
  exclusive    INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX idx_session_leases_heartbeat ON session_leases(heartbeat_at);
CREATE INDEX idx_session_leases_worker ON session_leases(worker_id);
```

### `workers`

```sql
CREATE TABLE workers (
  worker_id    TEXT PRIMARY KEY NOT NULL,
  session_id   TEXT NOT NULL UNIQUE
               REFERENCES sessions(id) ON DELETE CASCADE,
  project_key  TEXT NOT NULL,
  name         TEXT NOT NULL,
  purpose      TEXT NOT NULL DEFAULT '',
  model        TEXT,
  status       TEXT NOT NULL DEFAULT 'online',
               -- online | idle | busy | stale | offline
  context_pct  REAL,
  pid          INTEGER,
  hostname     TEXT,
  started_at   TEXT NOT NULL,
  heartbeat_at TEXT NOT NULL,
  metadata     TEXT
) STRICT;
CREATE INDEX idx_workers_project_status ON workers(project_key, status);
CREATE INDEX idx_workers_project_name ON workers(project_key, name);
CREATE INDEX idx_workers_heartbeat ON workers(heartbeat_at);
```

### `worker_messages`

```sql
CREATE TABLE worker_messages (
  id               TEXT PRIMARY KEY NOT NULL,
  project_key      TEXT NOT NULL,
  from_worker_id   TEXT NOT NULL,
  from_session_id  TEXT NOT NULL,
  to_worker_id     TEXT,
  to_session_id    TEXT NOT NULL,
  kind             TEXT NOT NULL,  -- prompt | response | notify
  status           TEXT NOT NULL,  -- queued | delivered | complete | error | timeout
  conversation_id  TEXT,
  parent_msg_id    TEXT,
  hops             INTEGER NOT NULL DEFAULT 0,
  payload          TEXT NOT NULL,  -- JSON
  created_at       TEXT NOT NULL,
  delivered_at     TEXT,
  completed_at     TEXT,
  error            TEXT
) STRICT;
CREATE INDEX idx_worker_msg_inbox
  ON worker_messages(to_session_id, status, created_at);
CREATE INDEX idx_worker_msg_project
  ON worker_messages(project_key, created_at);
CREATE INDEX idx_worker_msg_parent ON worker_messages(parent_msg_id);
```

Soft refs for from/to worker ids (mailbox outlives offline peers).

---

## Runtime

### Crate split

| Layer                                        | Owner                   |
| -------------------------------------------- | ----------------------- |
| Lease / registry / mailbox stores            | `elph-agent` `workers/` |
| Tools                                        | `elph-agent`            |
| Heartbeat, poller, inject, turn trigger, TUI | `coding-agent`          |
| Docs                                         | `docs/workers.md`       |

### Lifecycle

```text
open store → retention GC (protect leased)
  → open/create session
  → acquire lease (fail hard if live foreign)
  → register worker
  → heartbeat task + inbox poller
  → TUI worker indicator subscription
  → on drop/SIGINT: release lease, mark offline (bounded timeout)
```

### Tools

| Tool           | Behavior                           |
| -------------- | ---------------------------------- |
| `worker_list`  | Live peers for project             |
| `worker_send`  | Fire-and-forget → `queued`         |
| `worker_ask`   | Send + wait for response / timeout |
| `worker_get`   | Non-blocking poll of own `msg_id`  |
| `worker_await` | Blocking wait                      |
| `worker_reply` | Reply to inbound ask               |

System prompt: inbound auto-replies via normal assistant output; do not `worker_send` to reply.

### Settings

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
        "tuiShowPeers": true
    }
}
```

---

## TUI: multi-worker indicator (required)

### Visibility rule

| Live workers in project (status ∈ online/idle/busy, heartbeat fresh) | UI                           |
| -------------------------------------------------------------------- | ---------------------------- |
| **≤ 1** (only self, or alone)                                        | **Hidden** — no chrome noise |
| **≥ 2**                                                              | **Show** compact indicator   |

`tuiShowPeers: false` forces always hidden.

### Placement & content (minimal, chrome-fit friendly)

Prefer **footer right cluster** (or status row segment) next to model/mode, using existing progressive-fit chrome:

- Collapsed: `⬡ 3` or `workers · 3` (count of live peers **including self**, or peers-only — **recommend: total live in project**, so 2 means two terminals).
- Optional hover/expand later: not required v1.

When count drops to 1 after peer exit, **hide on next poll** (same interval as inbox/heartbeat refresh, or dedicated 1–2s peer refresh).

### Data source

- Read `workers` filtered by `project_key` + fresh `heartbeat_at`.
- Coding-agent holds `Arc` registry snapshot updated by heartbeat task (or light poll).
- **No** second DB open in render path; use shared handle.

### Non-goals for TUI v1

- Full pool widget with bars (coms-style) — later
- Alt+M compose overlay — later (`/workers` list-only ok)
- Network multi-host pool

### Phase placement

TUI lands in **PR3** (with registry live) or **PR4** latest — **not optional polish**; required for multi-worker UX. Until then CLI/`worker_list` still usable.

---

## Delivery phases

### PR1 — Lease (solid foundation)

- v202 schema (all three tables; messages unused until PR3 ok)
- `LeaseStore` acquire / heartbeat / release / reclaim
- Wire session open; clear error on conflict
- Tests: dual open same session fails; stale reclaim

### PR2 — Registry + tools list + TUI count

- Register worker; heartbeat updates status
- `worker_list`
- **TUI indicator: show iff live_count ≥ 2**
- Tests: two workers listed; demote stale

### PR3 — Mailbox + delivery (durable path)

- send / get / await / ask / reply
- Atomic claim; inject + idempotent `msg_id`
- Idle trigger / busy steer
- Auto-response on turn end for asks
- Tests: A→B deliver; ask round-trip; crash between claim and inject no double inject

### PR4 — Resilience hardening + docs

- Retention: protect leased sessions
- Sweep timeouts; hop enforcement
- Bounded shutdown
- `docs/workers.md` + settings docs
- Optional `/workers` slash

### Adjacent (parallel, not blocking)

- Durable agent mode; wire `session_entries.turn_id`

---

## Notify plane

| v1                        | Later                     |
| ------------------------- | ------------------------- |
| Poll only (`inboxPollMs`) | Socket fanout / WAL watch |
| Simple and correct        | Lower latency UX          |

---

## Non-goals (v1)

- Multi-machine SSE hub
- Broadcast chat room
- Dual-writer merge on one session
- Mid-stream provider resume
- Replacing in-process subagents

---

## Success criteria

1. Two terminals same project: mutual visibility; **footer shows worker count ≥2**; **hidden with one process**.
2. `worker_send` / `worker_ask` durable across receiver restart (queued mail).
3. Same session cannot be opened exclusively by two processes.
4. Stale lease reclaim after kill -9 + timeout.
5. No double inject of same `msg_id`.
6. GC does not delete leased sessions.
7. Docs match implementation.

---

## Implementation notes

- Module: `elph-agent/src/workers/{lease,registry,mailbox,tools,mod}.rs`
- Coding-agent: `WorkerRuntime` (heartbeat + poller + Drop)
- TUI: thin count badge in chrome fit pipeline
- Clean break ok for v202 additive tables
- Match `AGENTS.md` import/test conventions
