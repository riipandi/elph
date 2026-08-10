---
type: Workflow
title: Multi-Process Worker Coordination
description: Multi-process worker coordination in Elph — session leases, file leases, mailbox, worker registry, and agent tools
tags: [workers, multi-process, coordination, session-lease, file-lease, mailbox]
openwiki:
    roles: [architecture, workflow]
    source_paths:
        [
            crates/elph-agent/src/workers/,
            crates/coding-agent/src/agent/worker_runtime.rs,
        ]
    change_kinds: [lifecycle, public-api]
    symbols:
        [
            WorkerRuntime,
            WorkerRegistry,
            SessionLeaseStore,
            FileLeaseStore,
            MailboxStore,
            WorkerToolContext,
            WorkerStatus,
            LiveWorker,
            WorkerMessage,
            PathClaimContext,
        ]
    test_paths:
        [
            crates/elph-agent/src/workers/lease.rs,
            crates/elph-agent/src/workers/file_lease.rs,
        ]
    invariants:
        [
            One worker ID per process lifetime; session lease reclaim on dead PID; file lease content_hash detects concurrent edits; mailbox delivery atomic,
        ]
    validation_commands: [cargo test -p elph-agent --lib workers]
---

# Workers

Multi-process worker coordination allows multiple Elph processes to collaborate on the same project. Workers share a project store (`store.db`) for session leases, file leases, and mailbox messages. The worker system is an [Elph delta] — no pi equivalent.

## Module Structure

### Agent side (`crates/elph-agent/src/workers/`)

```
crates/elph-agent/src/workers/
├── mod.rs          — re-exports
├── registry.rs     — WorkerRegistry (project-scoped live worker table)
├── lease.rs        — SessionLeaseStore (exclusive session leases)
├── file_lease.rs   — FileLeaseStore (cross-process path claims)
├── mailbox.rs      — MailboxStore (durable worker messages)
├── path_claim.rs   — PathClaimContext (injected into FS mutate tools)
├── pid.rs          — pid_alive() (best-effort PID liveness check)
├── tools.rs        — WorkerToolContext, create_worker_tools() (5 tools)
└── types.rs        — WorkerStatus, WorkerRecord, WorkerMessage, FileLease
```

### Product side (`crates/coding-agent/src/agent/worker_runtime.rs`)

- `WorkerRuntime` — manages heartbeat, inbox polling, session lease, and file lease refresh
- `WorkerRuntimeStart` — config for starting the worker runtime

## Key Types

### WorkerStatus

```rust
pub enum WorkerStatus {
    Online, Idle, Busy, Stale, Offline,
}
impl WorkerStatus {
    pub fn is_live(self) -> bool { /* Online | Idle | Busy */ }
}
```

### WorkerRecord

```rust
pub struct WorkerRecord {
    pub worker_id: String,
    pub session_id: String,
    pub project_key: String,
    pub name: String,
    pub purpose: String,
    pub model: Option<String>,
    pub status: WorkerStatus,
    pub context_pct: Option<f64>,
    pub pid: Option<i64>,
    pub hostname: Option<String>,
    pub started_at: String,
    pub heartbeat_at: String,
}
```

### WorkerMessage

```rust
pub struct WorkerMessage {
    pub id: String,
    pub project_key: String,
    pub from_worker_id: String,
    pub to_worker_id: Option<String>,
    pub kind: MessageKind,       // Prompt | Response | Notify
    pub status: MessageStatus,   // Queued | Delivered | Complete | Error | Timeout
    pub hops: i64,
    pub payload: String,
    // ...
}
```

### FileLease

```rust
pub struct FileLease {
    pub project_key: String,
    pub path_norm: String,
    pub worker_id: String,
    pub session_id: String,
    pub mode: String,
    pub content_hash: Option<String>,
    // ...
}
```

## Worker Tools

Five agent tools created by `create_worker_tools()` in `workers/tools.rs`:

| Tool           | Type      | Description                       |
| -------------- | --------- | --------------------------------- |
| `worker_list`  | Read-only | List live peers (excludes self)   |
| `worker_send`  | Write     | Fire-and-forget message to a peer |
| `worker_get`   | Read-only | Non-blocking poll by msg_id       |
| `worker_await` | Read-only | Block until response or timeout   |
| `worker_ask`   | Write     | Send + block for reply            |

## Lifecycle

1. `WorkerRuntime::start()` — registers worker, starts heartbeat loop
2. Heartbeat loop refreshes session lease, file leases, marks stale peers, sweeps mailbox timeouts
3. `WorkerRuntime::shutdown()` — releases session lease, file leases, marks offline

### Session Lease Reclaim Rules

`SessionLeaseStore::try_acquire()`:

1. **Same worker ID** → re-entrant (allowed)
2. **Holder PID dead** → immediate reclaim (via `pid_alive()`)
3. **Holder PID alive** → conflict (returns `LeaseConflict`)

### File Lease Invariants

- `content_hash` detects concurrent edits
- `ensure_content_unchanged()` bails if hash mismatch
- Re-entrant for same worker (same session, same path)
- `PathClaimContext` is injected into FS mutate tools so they claim paths before writing

### Mailbox Delivery

- `claim_next_inbound()` marks delivered atomically before running the model, preventing replay loops
- `max_hops` prevents infinite forwarding chains
- `sweep_timeouts()` marks timed-out prompts project-wide

## Source References

- `crates/elph-agent/src/workers/mod.rs` — module re-exports
- `crates/elph-agent/src/workers/registry.rs` — `WorkerRegistry`, `register()`, `demote_stale()`, `list_live_peers()`
- `crates/elph-agent/src/workers/lease.rs` — `SessionLeaseStore`, `try_acquire()`, `release()`
- `crates/elph-agent/src/workers/file_lease.rs` — `FileLeaseStore`, `try_claim()`, `ensure_content_unchanged()`
- `crates/elph-agent/src/workers/mailbox.rs` — `MailboxStore`, `send_prompt()`, `claim_next_inbound()`, `send_response()`
- `crates/elph-agent/src/workers/path_claim.rs` — `PathClaimContext`, `SharedPathClaim`, `normalize_claim_path()`
- `crates/elph-agent/src/workers/pid.rs` — `pid_alive()`
- `crates/elph-agent/src/workers/tools.rs` — `WorkerToolContext`, `create_worker_tools()`
- `crates/elph-agent/src/workers/types.rs` — `WorkerStatus`, `WorkerRecord`, `WorkerMessage`, `FileLease`
- `crates/coding-agent/src/agent/worker_runtime.rs` — `WorkerRuntime`, `WorkerRuntimeStart`
