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

## Status audit (as of this revision)

### Done (library / schema)

| Piece               | Location                           | Notes                                                             |
| ------------------- | ---------------------------------- | ----------------------------------------------------------------- |
| Schema v202         | `WORKERS_SCHEMA_SQL`               | `session_leases`, `workers`, `worker_messages`, `file_leases`     |
| Platform migrations | coding-agent `platform/migrations` | versions 201+202                                                  |
| `SessionLeaseStore` | `elph-agent/workers/lease.rs`      | acquire / heartbeat / release; reclaim needs **stale + pid dead** |
| `WorkerRegistry`    | `workers/registry.rs`              | register, heartbeat, list live, demote stale                      |
| `MailboxStore`      | `workers/mailbox.rs`               | send / claim / complete / timeout                                 |
| `FileLeaseStore`    | `workers/file_lease.rs`            | claim / refresh / release / reclaim                               |
| Worker tools (defs) | `workers/tools.rs`                 | `worker_list`, `send`, `get`, `await`, `ask`                      |
| SessionManager hook | `with_session_lease`               | **never called** from runtime yet                                 |
| Unit tests          | lease store                        | migrations / unified_store expect tables                          |

### Not done (product path)

| Gap                                                     | Impact                                             |
| ------------------------------------------------------- | -------------------------------------------------- |
| Runtime never enables lease                             | Dual open same session still possible              |
| No register / heartbeat / Drop cleanup                  | Registry empty; leases go stale only after timeout |
| Worker tools not registered on harness                  | Agent cannot coordinate                            |
| No inbox poller / inject                                | Mailbox writes sit forever                         |
| File leases not on mutate tools                         | Shared-cwd silent overwrite still possible         |
| No TUI peer count                                       | Badge absent                                       |
| No `workers.*` settings                                 | Hardcoded defaults only if wired ad-hoc            |
| Retention GC ignores leases                             | Active peer session can be GC'd                    |
| Related: `turn_id` often NULL; agent mode process-local | Orthogonal durability gaps (track separately)      |

### Design bug to fix before wiring

`WorkerRegistry::register` **always** `create_worker_id()`.  
`SessionManager::with_session_lease` needs a **worker_id before** open.

**Rule:** one `worker_id` per process lifetime, generated once at session start, shared by:

- session lease
- workers row
- file_leases
- mailbox from_* fields

Change: `register(... worker_id: &str ...)` or `register_with_id`, do **not** mint a second id.

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
         sessions · session_entries · turns
         session_leases · workers · worker_messages · file_leases
```

- **Subagent** = in-process, same session tree.
- **Worker** = separate OS process + own session + mailbox + optional shared cwd file claims.

---

## Durability / solid / resilient (unchanged requirements)

### Durable SoT

| State               | Table             |
| ------------------- | ----------------- |
| Transcript          | `session_entries` |
| Session exclusivity | `session_leases`  |
| Presence            | `workers`         |
| Mail                | `worker_messages` |
| Path claims         | `file_leases`     |

### Solid concurrency

| Domain       | Rule                                                             |
| ------------ | ---------------------------------------------------------------- |
| Session tree | One exclusive lease per `session_id`                             |
| Files        | One write claim per `(project_key, path_norm)`                   |
| Inbox claim  | Atomic `queued` → `delivered` (rows_affected = 1)                |
| File claim   | Atomic insert / same-worker refresh; foreign live holder → error |

### Resilient failures

| Failure                 | Recovery                                      |
| ----------------------- | --------------------------------------------- |
| Receiver offline        | Mail stays `queued`; poll delivers later      |
| Double inject           | Idempotent by `msg_id` on custom entry        |
| Hard kill holding lease | Reclaim when heartbeat stale **and** pid dead |
| Missed notify           | Poll recovers                                 |
| File claim after kill   | Same stale window as session lease            |
| Hash / content race     | Mutate tool fails; no silent overwrite        |

**Reclaim policy (keep):** stale alone is not enough if pid still alive (hung process). Document that operator may need to kill pid or wait for process death.

---

## Parallel file edit (defense in depth)

| Layer                   | Mechanism                                                                  |
| ----------------------- | -------------------------------------------------------------------------- |
| 0 Isolation (preferred) | Separate git worktrees → different `project_key` → no file_lease collision |
| 1 Anticipation          | `file_leases` before mutate                                                |
| 2 Detection             | Content hash / existing edit preconditions                                 |
| 3 Resolution            | Actionable tool error (holder name + purpose); optional `worker_ask`       |
| 4 Social                | Prompt + `worker_list` (claims summary optional)                           |

**Default:** `fileLeases` on when workers enabled. Opt-out is explicit and dangerous for shared cwd.

**Claim lifetime (v1 decide):** keep until **session end** (or process Drop), not turn end — simpler, fewer thrash; heartbeat refreshes. Optional later: release after N idle turns.

Mutate tools to wrap: `edit_file`, `write_file`, `delete_path`, `move_path`, dest of `copy_path` (and any FS MCP write aliases if present).

---

## Runtime lifecycle (single integration spine)

```text
ensure_database (v202)
  → optional GC (protect leased sessions — PR-H)
  → worker_id = create_worker_id()
  → SessionManager::new_with_database(...).with_session_lease(worker_id, stale)
  → create/open session  // acquire lease or fail with clear message
  → WorkerRegistry::register_with_id(worker_id, session_id, project_key, name, …)
  → start WorkerRuntime:
        • heartbeat loop (session_lease + worker + file_leases)
        • inbox poller (claim → inject custom entry → optional auto-steer)
        • live peer count → TUI channel
  → register create_worker_tools if enabled
  → FileLeaseStore injected into BuiltinTools / mutation path
  → Drop / graceful exit:
        release file leases → release session lease → mark worker offline
        (best-effort, timeout-bounded)
```

### `WorkerRuntime` (new in coding-agent)

Thin host struct holding:

- `worker_id`, `session_id`, `project_key`
- `Arc` stores (lease, registry, mailbox, file)
- `JoinHandle`s for heartbeat + inbox
- `watch`/`mpsc` for `live_peer_count: u32`
- `shutdown()` used from session Drop / quit path

Keep **logic** in `elph-agent`; **wiring** in coding-agent runtime/TUI.

---

## Settings (`workers` group)

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

- Defaults above; `enabled: false` skips register/tools/heartbeat but schema remains.
- Prefer **on by default**: single-user sees no badge (count ≤ 1); zero UX cost.
- Name: settings / env / hostname-derived / `worker-{shortid}`.

---

## TUI badge

| Live workers (fresh heartbeat, online/idle/busy) | UI                                            |
| ------------------------------------------------ | --------------------------------------------- |
| ≤ 1                                              | **Hidden**                                    |
| ≥ 2                                              | Compact chrome/footer: `⬡ N` or `workers · N` |

- Source: `WorkerRuntime` peer count poll (~1–2s or on heartbeat).
- `tuiShowPeers: false` → always hide.
- Placement: prompt footer or header chrome (match density; avoid second permanent row).

---

## Tools (agent-facing)

| Tool                                         | Role                                                             |
| -------------------------------------------- | ---------------------------------------------------------------- |
| `worker_list`                                | Peers; optional path-claim summary later                         |
| `worker_send`                                | Fire-and-forget (not for replies)                                |
| `worker_ask` / `worker_get` / `worker_await` | Request + poll                                                   |
| (no `worker_reply`)                          | Answer inbound in normal assistant text; host auto-completes ask |

Inbox inject:

1. Claim `queued` → `delivered`
2. Append session custom entry `worker.inbound` with `msg_id`
3. Skip inject if `msg_id` already in branch
4. If agent idle → trigger turn; if busy → steer next turn / queue

---

## Delivery phases (revised)

Foundation libraries are **done**. Remaining work is **host integration** + product polish.  
Rebase old PR1–5 onto this spine (no re-implement of stores).

### PR-A — Runtime lease + registry + heartbeat **[critical path]**

1. Fix `register` to accept existing `worker_id`.
2. Wire `create_coding_session_with_events`: generate id → `with_session_lease` → open → `register_with_id`.
3. `WorkerRuntime` heartbeat (session + worker).
4. Release lease + offline worker on Drop/quit.
5. Integration test: second process open same session → conflict error.
6. Same project two sessions → both succeed.

**Exit criteria:** dual-open blocked; kill + stale window reclaims; clean exit releases immediately.

### PR-B — Tools + TUI badge

1. Settings `workers.*` (minimal: enabled, name, heartbeat, stale, tuiShowPeers).
2. Register `create_worker_tools` when enabled.
3. Peer count into TUI; badge only if **≥ 2**.
4. Unit/UI smoke: count 1 hidden, count 2 visible.

**Exit criteria:** two terminals show badge; alone no badge; `worker_list` returns peer.

### PR-C — File leases on mutate tools

1. Inject `FileLeaseStore` + worker identity into FS mutate path (alongside in-process `FileMutationQueue`).
2. Claim before write; clear conflict errors.
3. Heartbeat refreshes owned claims; Drop releases all.
4. Test: two workers, same path → second fails; disk unchanged; stale reclaim then succeeds.

**Exit criteria:** shared-cwd parallel edit safe by default.

### PR-D — Mailbox inbox + ask completion

1. Inbox poller in `WorkerRuntime`.
2. Idempotent inject + idle trigger / busy queue.
3. On turn end: complete open `ask`s from assistant text (or explicit policy).
4. Timeout sweeper for `askTimeoutMs`.
5. Test A→B send and ask round-trip across two DBs sessions.

**Exit criteria:** durable mail; restart receiver still delivers; hop limit enforced.

### PR-E — Hardening + docs

1. Retention/GC: **never delete leased** (or live-heartbeat) sessions.
2. Prompt snippet when `workers.enabled` (path ownership, worktrees, no silent overwrite, never `worker_send` as reply).
3. `docs/workers.md` + refresh `docs/planned/agent-orchestration-v2.md` status.
4. Optional `/workers` slash later (not blocking).
5. Align related durability: wire `turn_id` on turn inserts (small follow-up if not blocking multi-worker).

---

## Non-goals (v1)

- CRDT / auto-merge concurrent edits
- Whole-repo exclusive lock
- Multi-machine fleet
- Cross-worktree `file_leases` (different `project_key` by design)
- Replacing subagents
- Directory-prefix claims (v1.1)

---

## Success criteria

1. Two terminals, same project: badge ≥2; alone hidden.
2. Same session cannot dual-open while lease live.
3. Same path mutation by two workers: second fails with holder; disk unchanged.
4. Stale file/session lease reclaimed after kill + stale window + dead pid.
5. Mailbox survives receiver restart.
6. Ask round-trip works; hop limit enforced.
7. GC does not delete leased/live sessions.
8. Docs: worktree isolation + file leases + settings.

---

## Implementation notes

| Area        | Guidance                                                                      |
| ----------- | ----------------------------------------------------------------------------- |
| Crates      | Logic in `elph-agent::workers`; host in coding-agent `runtime` + TUI          |
| DB handle   | Always `.with_database(shared Arc)` — same as GoalStore/memory                |
| project_key | Normalize cwd the same way as session manager                                 |
| Path norm   | Reuse existing tool path normalization (project-relative)                     |
| Heartbeat   | One task, one interval: lease + worker + file_leases                          |
| Errors      | User-facing: short, actionable (holder pid/name/session)                      |
| Compat      | No legacy data migration; rebuild/wipe OK per project doctrine                |
| Tests       | Unit in store files; integration under coding-agent `tests/` for dual-session |
| Docs        | Significant → update `docs/` before done (`AGENTS.md`)                        |

---

## Out of band (do not block multi-worker)

- Persist agent mode in session metadata (today process-local).
- Ensure `turn_id` set on `session_turns` / related inserts.
- Directory-level file claims.
- Explicit `file_claim` / `file_release` tools.

---

## Suggested implementation order (this revision)

```text
PR-A (lease+registry+heartbeat) → PR-B (tools+TUI) → PR-C (file leases)
  → PR-D (mailbox inject) → PR-E (GC protect + docs)
```

Do **not** re-land schema or rewrite stores unless a wiring bug forces it.
