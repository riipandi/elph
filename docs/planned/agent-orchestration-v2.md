# Plan (revisi 2): Multi-Worker — Runtime Integration

## Goal

Multiple Elph processes on one project as peer workers (**1 process ≈ 1 agent/session**):

1. **Durable** registry + mailbox in project DB (notify never SoT)
2. **Solid** exclusive session leases (no dual writers on one session tree)
3. **Resilient** poll recovery + stale reclaim (heartbeat + pid-dead rules)
4. **Parallel file-edit safety** via `file_leases` on mutate tools
5. **TUI** peer badge when **≥2 live workers**; **hidden when alone**

v1 = same machine, shared `.elph/store.db`. Multi-host hub out of scope.

---

## Status (implementation)

| Area | Status |
| --- | --- |
| Schema v202 (`session_leases`, `workers`, `worker_messages`, `file_leases`) | **Done** |
| Stores + worker tool defs (`elph-agent::workers`) | **Done** |
| Shared `worker_id` for lease + registry (`register(worker_id, …)`) | **Done** |
| Runtime: lease on open + register + heartbeat + Drop cleanup | **Done (PR-A)** |
| Settings `workers.*` + agent tools registered | **Done (PR-B)** |
| TUI badge ≥2 (`⬡ N`) | **Done (PR-B)** |
| File leases on mutate tools | **Done (PR-C)** |
| Mailbox inbox inject + steer | **Done** |
| Ask auto-complete on turn end + timeout sweep | **Done** |
| Memorable-id worker names + pid demote reaper | **Done** |
| Graceful quit → `shutdown_workers` | **Done** |
| Multi-worker prompt snippet | **Done** |
| File claim content fingerprint | **Done** |
| Integration tests (`workers_multi`) | **Done** |
| Retention protect leased + `docs/workers.md` | **Done (PR-E)** |

---

## Mental model

```text
 Terminal A: elph                      Terminal B: elph
 session S_a + lease(worker_a)         session S_b + lease(worker_b)
 registry row · heartbeat task         registry row · heartbeat task
 file_leases for paths it mutates      file_leases for paths it mutates
        │                                     │
        └──────────────────┬──────────────────┘
                           ▼
                    .elph/store.db
```

- **Subagent** = in-process, same session tree.
- **Worker** = separate OS process + own session + mailbox + optional shared cwd file claims.

**One `worker_id` per process lifetime** — used for session lease, `workers` row, file claims, mailbox `from_*`.

---

## Durability / solid / resilient

| Concern | Rule |
| --- | --- |
| SoT | DB tables only; notify/TUI never SoT |
| Session tree | Exclusive `session_leases`; reclaim when heartbeat stale **and** pid dead |
| Files | One write claim per `(project_key, path_norm)` |
| Inbox | Atomic `queued` → `delivered`; inject idempotent by `msg_id` |

---

## Parallel file edit (PR-C)

Defense in depth: worktree isolation (preferred) → path claims → hash/precondition → clear tool errors → social via `worker_list` / `worker_ask`.

Claim lifetime v1: until session end / process Drop; heartbeat refreshes.

---

## Delivery phases

### PR-A — Runtime lease + registry + heartbeat ✅ (in progress / landed)

1. `register` accepts existing `worker_id`
2. `create_coding_session_with_events`: mint id → `with_session_lease` → open → `WorkerRuntime::start`
3. Heartbeat: session lease + worker (+ file leases when present)
4. Drop / `shutdown_workers`: release file leases, session lease, mark offline

### PR-B — Tools + TUI badge

Settings `workers.*`; register `create_worker_tools`; peer count badge only if **≥ 2**.

### PR-C — File leases on mutate tools

Wire claim into edit/write/delete/move/copy dest.

### PR-D — Mailbox inbox + ask completion

Poller, inject, idle trigger, ask timeout sweeper.

### PR-E — Hardening + docs

GC protect leased sessions; prompt snippet; `docs/workers.md`.

---

## Settings (target)

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

---

## Non-goals (v1)

- CRDT / auto-merge concurrent edits
- Whole-repo exclusive lock
- Multi-machine fleet
- Cross-worktree `file_leases`
- Replacing subagents

---

## Code map

| Piece | Path |
| --- | --- |
| Schema | `elph-agent/src/session/migrations.rs` (`WORKERS_SCHEMA_SQL`) |
| Stores | `elph-agent/src/workers/` |
| Host lifecycle | `coding-agent/src/agent/worker_runtime.rs` |
| Session open | `coding-agent/src/agent/runtime.rs` + `session_manager.rs` |
| Session accessors | `CodingAgentSession::worker_live_count`, `shutdown_workers` |
