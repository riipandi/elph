# Plan (revisi): Multi-Worker + Parallel File Edit Safety

## Goal

Enable **multiple Elph processes on one project** as peer workers (**1 process ≈ 1 agent/session**), with:

1. **Durable** mailbox + registry in project DB
2. **Solid** exclusive **session** leases (no dual writers on one session tree)
3. **Resilient** delivery (poll recovers; notify ≠ SoT)
4. **Parallel file-edit safety**: anticipate, detect, and resolve conflicts when workers share a worktree
5. **TUI** multi-worker indicator when **≥2 live workers**; **hidden when only one**

v1 = same machine, shared `.elph/store.db`. Multi-host hub out of scope.

---

## Problem: parallel file editing

Today:

- `FileMutationQueue` serializes mutations **inside one process only**.
- Two Elph instances on the **same cwd** can `edit_file` / `write_file` the same path → last write wins, silent corruption.
- Session lease does **not** protect the filesystem.

Multi-worker without file coordination is **not** production-safe for shared worktrees.

### Strategy layers (defense in depth)

| Layer                        | Mechanism                                                               | When                          |
| ---------------------------- | ----------------------------------------------------------------------- | ----------------------------- |
| **0. Isolation (preferred)** | Separate **git worktrees** per worker (`elph worktree` already exists)  | Heavy parallel implementation |
| **1. Anticipation**          | Path **claims / file leases** in DB before mutating                     | Default shared-cwd fleet      |
| **2. Detection**             | Precondition: content hash / mtime before apply                         | Every edit/write              |
| **3. Resolution**            | Fail tool clearly; optional `worker_ask` / user; never silent overwrite | On conflict                   |
| **4. Social**                | System prompt + `worker_list` awareness of claims                       | Soft                          |

**Default product posture:** shared cwd allowed **with** leases + hash checks; recommend worktree isolation for large parallel tasks in docs/prompt.

---

## Durability / solid / resilient (non-negotiable)

### Durable SoT (DB)

| State               | Table / place     |
| ------------------- | ----------------- |
| Session transcript  | `session_entries` |
| Session exclusivity | `session_leases`  |
| Worker presence     | `workers`         |
| Inter-worker mail   | `worker_messages` |
| Path claims         | `file_leases`     |

Notify / sockets / TUI never SoT.

### Solid concurrency

| Domain        | Rule                                                                           |
| ------------- | ------------------------------------------------------------------------------ |
| Session tree  | One exclusive lease per session                                                |
| Files         | At most one exclusive **write** lease per absolute/normalized path per project |
| Message claim | Atomic `queued` → `delivered`                                                  |
| File claim    | Atomic INSERT or CAS on lease row                                              |

### Resilient failure modes

| Failure                        | Recovery                                                |
| ------------------------------ | ------------------------------------------------------- |
| Worker dies holding file lease | Reclaim if heartbeat/lease stale (+ optional pid dead)  |
| Edit after another writer      | Hash mismatch → tool error, no write                    |
| Stale claim after kill -9      | Same reclaim window as session lease (`leaseStaleSecs`) |
| Two claim races                | UNIQUE(path) / PK path; loser retries or fails          |
| Mail lost?                     | Impossible if insert committed before notify            |
| Double inject message          | Idempotent by `msg_id` on session custom entry          |

---

## Parallel file edit design

### Schema: `file_leases`

```sql
CREATE TABLE file_leases (
  project_key   TEXT NOT NULL,
  path_norm     TEXT NOT NULL,          -- canonical relative or abs key
  worker_id     TEXT NOT NULL,
  session_id    TEXT NOT NULL,
  mode          TEXT NOT NULL DEFAULT 'write',  -- write | exclusive
  purpose       TEXT,                   -- short claim reason
  content_hash  TEXT,                   -- hash at claim time (optional)
  acquired_at   TEXT NOT NULL,
  heartbeat_at  TEXT NOT NULL,
  expires_at    TEXT,                   -- optional hard TTL
  PRIMARY KEY (project_key, path_norm)
) STRICT;
CREATE INDEX idx_file_leases_worker ON file_leases(worker_id);
CREATE INDEX idx_file_leases_heartbeat ON file_leases(heartbeat_at);
CREATE INDEX idx_file_leases_session ON file_leases(session_id);
```

No FK to `workers` required (survive offline reclaim); `session_id` soft-ref or FK CASCADE if session deleted.

### Acquire / release API (`elph-agent`)

```text
try_claim_path(project, path, worker_id, session_id, purpose) -> Ok | Conflict(holder)
refresh_file_leases(worker_id)   -- with session heartbeat
release_path / release_all(worker_id | session_id)
reclaim_stale(leaseStaleSecs)
```

### Wire into mutating tools (coding-agent / elph-agent tools)

On **`edit_file` / `write_file` / `delete_path` / `move_path` / `copy_path` (dest)**:

1. Normalize path (project-relative key).
2. **`try_claim_path`** if not already held by this worker.
3. Read current file hash (if exists).
4. If claim held by **other** live worker → **tool error** with holder name + purpose (actionable).
5. Apply mutation.
6. Keep claim until turn end **or** explicit release; heartbeat refreshes claims.

Optional v1.1: claim **directory prefix** for multi-file features (careful with over-locking).

### Precondition hash (detect)

Even with claim:

- `edit_file`: if on-disk content no longer matches expected base (or hash at claim), fail with **conflict** rather than corrupting.
- Align with existing edit semantics (unique old_string already fails sometimes).

### Resolution playbook (agent + user)

| Situation            | Behavior                                                                             |
| -------------------- | ------------------------------------------------------------------------------------ |
| Claim conflict       | Tool returns holder worker name; model should `worker_ask` or wait / pick other path |
| Hash conflict        | Tool fails; model re-reads file, re-plans                                            |
| Shared cwd risk high | Prompt: prefer worktree or split path ownership                                      |
| User override        | Optional `force: true` on tools (settings-gated) — **default off**                   |

### Isolation path (recommended for heavy parallel)

Document + optional host helper:

```text
elph worktree create worker-a
cd … && elph   # separate cwd → separate project store OR shared repo different worktree path
```

If worktrees share one git object store but **different cwd**, `project_key` differs → **no file_lease collision** (correct: different trees). Same store only if same project dir.

Clarify: **file leases are per `project_key` (= session cwd key)**. Two worktrees = two project keys = no cross-lease (by design). Coordination across worktrees uses **git** + `worker_*` messages, not file_leases.

---

## Schema v202 (full additive set)

1. `session_leases` — exclusive session writer
2. `workers` — live registry
3. `worker_messages` — durable mailbox
4. `file_leases` — path claims

Indexes + `PRAGMA foreign_keys=ON` as today.

---

## Runtime (summary)

### Lifecycle

```text
open store → GC (protect leased sessions)
  → open session → acquire session_lease
  → register worker
  → heartbeat (session lease + worker + owned file_leases)
  → inbox poller + file-lease stale reclaim
  → TUI peer count
  → Drop: release session lease, file leases, mark worker offline
```

### Tools

| Tool                                              | Role                                          |
| ------------------------------------------------- | --------------------------------------------- |
| `worker_list`                                     | Peers (+ optional active path claims summary) |
| `worker_send` / `ask` / `get` / `await` / `reply` | Mailbox                                       |
| (internal) path claim via mutating tools          | Not necessarily a separate tool in v1         |

Optional later: `file_claim` / `file_release` tools for explicit multi-step ownership.

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
        "tuiShowPeers": true,
        "fileLeases": true,
        "fileLeaseOnMutate": true
    }
}
```

`fileLeases: false` = opt-out (dangerous shared cwd); default **true**.

---

## TUI multi-worker indicator

| Live workers (fresh heartbeat, status online/idle/busy) | UI                                                  |
| ------------------------------------------------------- | --------------------------------------------------- |
| **≤ 1**                                                 | **Hidden**                                          |
| **≥ 2**                                                 | Footer/status compact badge: `⬡ N` or `workers · N` |

Optional micro-hint when **any** `file_leases` held by peers: not required v1 (badge enough).  
`tuiShowPeers: false` → always hide.

---

## Delivery phases

### PR1 — Session lease

- Tables: at least `session_leases` (+ stub others if preferred single migration)
- Acquire / heartbeat / release / reclaim
- Block dual open same session
- Tests

### PR2 — Workers registry + TUI count

- `workers` register/heartbeat/stale
- `worker_list`
- **TUI badge if count ≥ 2**
- Tests

### PR3 — File leases + tool wiring (parallel edit)

- `file_leases` store
- Wire mutate tools: claim + hash check + clear errors
- Heartbeat refresh + stale reclaim + release on session end
- Tests: two workers, second edit same path fails with holder info
- Prompt note on shared cwd

### PR4 — Mailbox + coordination

- `worker_messages` + send/ask/get/await/reply
- Atomic claim, inject idempotent, idle trigger / busy steer
- Auto-response on turn end for asks
- Tests A→B

### PR5 — Hardening + docs

- Retention protect leased
- Timeout sweeper
- `docs/workers.md` (leases, file claims, worktree isolation)
- Optional `/workers`, claim listing in `worker_list` details

---

## Prompt / social protocol (coding_base snippet)

When `workers.enabled`:

- Prefer **non-overlapping paths**; claim via mutate tools automatically.
- On claim conflict: coordinate with holder via `worker_ask` or pick another file.
- Large parallel features: use **separate worktrees**, not one dirty tree.
- Never silent overwrite; never reply to inbound via `worker_send`.

---

## Non-goals (v1)

- CRDT / automatic 3-way merge of concurrent edits
- Global repo lock (whole tree exclusive to one worker)
- Multi-machine fleet
- Cross-worktree file_lease (different project_key)
- Replacing subagents

---

## Success criteria

1. Two terminals, same project: badge when ≥2; hidden alone.
2. Same session cannot be dual-opened.
3. Two workers, same path: second mutation fails with holder name; disk unchanged.
4. Stale file lease reclaimed after kill + timeout; then claim succeeds.
5. Mailbox durable across receiver restart.
6. Ask round-trip works; hop limit enforced.
7. Docs cover worktree isolation + file leases.

---

## Implementation notes

- `elph-agent/src/workers/{lease,registry,mailbox,file_lease,tools,mod}.rs`
- Mutating tools take optional `FileLeaseStore` / claim hook (host injects)
- Reuse path normalization from existing tools
- Heartbeat one task updates session + worker + file leases
- Match `AGENTS.md` conventions; no legacy data migration
