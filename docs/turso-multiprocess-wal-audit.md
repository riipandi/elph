# Turso Integration and Multiprocess WAL Audit

**Status:** Multiprocess-WAL hardening is implemented for shared opening, connection ownership, serialized transaction cleanup, atomic migration application with SQL checksums, session-leaf compare-and-swap, transactional todo merging, and a real two-process writer test. The project remains on `turso = 0.8.0-pre.4` and uses serialized `BEGIN IMMEDIATE` writes, not MVCC.

The current hardening pass validates durable filesystem paths before enabling multiprocess WAL and removes `experimental_vacuum` from shared database builders. In-place `VACUUM` requires exclusive ownership of the multiprocess WAL, so maintenance must be excluded from these shared open paths rather than enabled by default. Turso 0.8.0-pre.4 performs platform/filesystem capability checks during `Builder::build`; the application guard rejects URI and in-memory paths early.

Cross-process crash/reopen, checkpoint, schema-refresh, vacuum-exclusion, and unsupported-filesystem tests remain deployment validation work.

**Scope:** `crates/elph-agent`, `crates/coding-agent`, `crates/floppy`, workspace dependency configuration, database opening, migrations, transactions, session persistence, worker coordination, memory stores, WAL sidecars, and multiprocess access.

**Turso dependency:** `turso = 0.8.0-pre.4`

## Executive summary

The project enables Turso's experimental multiprocess WAL mode consistently on its primary local database paths. The high-level architecture is sound: the host opens one `Database`, shares it through `Arc<Database>`, and stores sessions, goals, todos, workers, and memory data in `.elph/store.db`.

The integration is not yet safe to classify as production-ready for concurrent OS processes. The main risks are:

1. Sidecar cleanup uses WAL modification time as a proxy for process liveness.
2. WAL recovery can remove an active `-wal` file and treats generic open errors as WAL corruption.
3. ~~The transaction helper is named and documented as MVCC but uses serialized `BEGIN IMMEDIATE`.~~ **Resolved:** the helper is `with_write_transaction` and the conflict classifier is `is_write_conflict_err`; no Turso MVCC is enabled (MVCC is incompatible with multiprocess WAL).
4. Lock retry does not cover all transaction phases, especially `BEGIN IMMEDIATE`.
5. No true two-process integration test proves that the Rust SDK can open and write the same file concurrently.
6. Session indexes are cached in memory and can become stale when another process writes the same session.
7. Migration check, DDL, and migration-ledger insertion are not one atomic operation.
8. Several read-modify-write workflows have TOCTOU or lost-update risks.
9. `elph-agent` and `floppy` duplicate Turso helpers with different behavior.
10. The dependency is a pre-release, while upstream has recently fixed multiprocess WAL locking and race issues.

**Verdict:** The builder flag is used in the correct places, but the surrounding integration requires hardening before relying on it for important persistent data across multiple processes.

## Implementation status

Implemented in the current hardening pass:

- Removed automatic deletion of `-wal`, `-shm`, and `-tshm` sidecars from the `elph-agent` and `floppy` open paths. Turso owns sidecar lifecycle.
- Removed WAL recovery based on file size and generic `unable to open database file` messages.
- Unified connection configuration so `floppy` also applies `PRAGMA foreign_keys = ON`.
- Renamed the transaction helper from `with_mvcc_transaction` to `with_write_transaction` and documented the actual serialized `BEGIN IMMEDIATE` model.
- Added bounded retry when acquiring `BEGIN IMMEDIATE`; commit failures remain terminal and are not blindly replayed.
- Require rollback to succeed before replaying a transient transaction error; commit failures perform best-effort rollback before returning.
- Made session physical prune update `active_leaf_id` in the same transaction as deletes and rollups.
- Added compare-and-swap protection for session `active_leaf_id`; stale independent writers now receive a conflict instead of silently overwriting the pointer.
- Made migration application and migration-ledger writes atomic and added SHA-256 checksums for migration SQL.
- Made todo merge read, merge, delete, and reinsert run under one serialized write transaction, preventing the previous cross-transaction lost-update window.
- Added a real two-process Rust test that opens and writes the same database file with multiprocess WAL enabled.
- Removed obsolete sidecar-recovery tests.
- Wrapped the `elph-agent` migration runner in the serialized write helper.

Not yet implemented in this pass:

- Crash/reopen, checkpoint, schema-refresh, and vacuum-exclusion integration tests.
- Runtime filesystem capability detection or a fallback for unsupported distributed filesystems.
- A shared helper module between `elph-agent` and `floppy`; their standalone APIs still live in separate crates.
- Explicit commit-outcome/idempotency reporting for failures where the server status is unknown is available through `CommitOutcome`; callers must still use an idempotency key or uniqueness constraint before retrying `Unknown`.
- Worker registration now allocates the display name inside the same serialized transaction as stale-row removal and insertion, closing the cross-process name-selection race.


### Shared host database

`crates/coding-agent/src/platform/datastore/mod.rs` opens `.elph/store.db` with:

```rust
.experimental_multiprocess_wal(true)
.experimental_index_method(true)
```

The returned `Database` is intended to be wrapped in `Arc` and shared with:

- `TursoSessionRepo`
- `GoalStore`
- `TodoStore`
- `WorkerRegistry`
- `SessionLeaseStore`
- `MemoryStore`

This is the preferred in-process model because it avoids creating multiple database authorities for the same file.

### Standalone paths

When a store does not receive an injected `Arc<Database>`, it opens the path through the shared helper in `elph-agent` or through `floppy`'s local database helper. These paths also enable multiprocess WAL.

### Duplicated helpers

There are two substantially similar implementations:

- `crates/elph-agent/src/datastore/conn.rs`
- `crates/floppy/src/core/db.rs`

Both implement multiprocess WAL, busy timeout, lock retry, sidecar cleanup, and WAL recovery. They are not behaviorally identical:

- `elph-agent` enables `foreign_keys` on connections; `floppy` does not.
- `floppy` enables `experimental_vacuum` for some paths.
- Retry limits and logging differ.
- Error and recovery handling differ.

This creates integration drift for a database shared by both layers.

## Multiprocess WAL findings

### Correct flag usage

The project consistently uses:

```rust
Builder::new_local(path)
    .experimental_multiprocess_wal(true)
```

This matches Turso documentation. All participating processes must use the same mode; the project follows this requirement in its internal open helpers.

### Missing true multiprocess coverage

The test named `concurrent_writers_dont_deadlock` uses `tokio::spawn` and therefore exercises multiple tasks in one process, not multiple OS processes. It does not prove:

- a second process can open a database held by the first;
- cross-process writer ownership;
- cross-process reader snapshots;
- cross-process checkpointing;
- schema refresh between processes;
- WAL lock behavior in the Rust SDK;
- crash and reopen behavior.

Turso upstream has had relevant Rust SDK issues, including the WAL file lock fix tracked by PR #7350 and subsequent follow-up work referenced as #7809. The changelog also records multiprocess binding races and rejection of MVCC/multiprocess WAL co-enablement. The exact contents of `0.8.0-pre.4` should be verified against those fixes.

## Sidecar and recovery findings

### Unsafe liveness heuristic

Both helper layers use the age of `<db>-wal` to infer whether a database is in use:

```rust
modified.elapsed() < Duration::from_secs(30)
```

This cannot prove that no process is using the database. A process can remain open while idle, a reader can hold a snapshot without changing the WAL, and system clock behavior can invalidate the assumption.

The open/configuration layer now removes sidecar cleanup and recovery heuristics, so the former sidecar findings no longer describe the current implementation. The remaining concern is operational: Turso still owns the experimental `.tshm` lifecycle, and no application code should delete these files while a process may be active.

### Cleanup occurs before every open

`open_local_internal` calls shared-memory cleanup before opening the database. `floppy` also calls `clear_broken_wal_sidecars` before opening. A normal open can therefore perform destructive sidecar maintenance while another process may still be active.

**Severity: P0.**

### Active WAL may be deleted

The recovery code removes `-wal` when its size is below 32 bytes, without a reliable active-process lock check. A short file can represent an in-progress initialization or write, not only corruption.

The classifier also treats the generic message `unable to open database file` as a WAL I/O error. That message can represent permissions, path, filesystem, descriptor, or lock failures and is not sufficient justification for deleting sidecars.

`floppy` additionally ignores removal errors in some paths, which can hide failed recovery.

**Severity: P0/P1 depending on deployment concurrency.**

## Transaction findings

### The helper is not MVCC

Previous revisions used a `with_mvcc_transaction` name and a public `is_mvcc_conflict_err` classifier that implied Turso MVCC. Both were renamed to reflect the actual model:

```sql
BEGIN IMMEDIATE
```

This is the standard WAL transaction mode that acquires a writer lock immediately. Turso MVCC requires `PRAGMA journal_mode = mvcc` and `BEGIN CONCURRENT`. Turso documents multiprocess WAL and MVCC as incompatible modes.

The current behavior is therefore:

```text
multiprocess WAL + serialized BEGIN IMMEDIATE
```

The transaction helper is now `with_write_transaction`, and the conflict classifier is `is_write_conflict_err` (classifies the serialized-writer-slot conflict surface, since MVCC is intentionally not enabled). The open path uses `is_open_retryable`, which retries on lock errors and Turso's "already open" multiprocess-WAL authority messages so a second Elph instance can absorb contention instead of failing fast.

### Retry coverage is incomplete

The retry loop begins after `BEGIN IMMEDIATE`. If acquiring the writer lock fails, the function returns immediately after the busy timeout. Commit errors also return immediately without a retry strategy.

A robust transaction policy must distinguish:

- begin contention;
- closure failure and rollback;
- commit failure;
- I/O or durability errors that must not be blindly replayed.

## Session persistence findings

### Sequence allocation

`allocate_seq` uses:

```sql
UPDATE session_sequences
SET next_seq = next_seq + 1
WHERE session_id = ?
RETURNING next_seq
```

This is preferable to a separate read and update, and it runs inside `BEGIN IMMEDIATE`. The local sequence allocation design is reasonable.

### Stale in-memory session indexes

`TursoSessionStorage::open` loads all entries and constructs an in-memory index. Later reads use that cached index. If another process appends to the same session, the first process does not refresh its index before creating IDs, validating leaf targets, or appending new entries.

The database sequence remains physically safe, but the logical tree state can become stale. `active_leaf_id` is shared mutable state without compare-and-swap semantics.

### Lost update on `active_leaf_id`

Two processes can both read the same old leaf and then commit different new leaves. The last update wins:

```sql
UPDATE sessions SET active_leaf_id = ? WHERE id = ?
```

This preserves transaction atomicity but does not enforce a semantic merge or expected-old-leaf check.

### Physical prune split across transactions

`physical_prune_except` commits deletes and rollups first, then persists the resulting leaf outside that transaction. A crash or concurrent writer between these phases can leave a stale active leaf pointer. The open-time self-healing path improves availability but does not provide atomic logical pruning.

### Best-effort stale-leaf healing

Healing can succeed in memory while persistence fails. The database and process-local index then temporarily disagree, and multiple processes may independently attempt repairs.

## Migration findings

### Migration check and application are not atomic

The migration runner performs:

1. check `app_migrations`;
2. execute migration DDL;
3. insert the migration ledger row.

These operations are not wrapped in one transaction. Two processes starting concurrently can both observe a missing version, execute overlapping DDL, and race on schema objects or the unique ledger index.

### Destructive schema rebuild

Migration v201 drops and recreates session-domain tables. The ledger is recorded only after the multi-statement batch finishes. A process termination during the batch can leave a partial schema, with no migration lock, recovery marker, or post-migration validation.

### Ledger has no migration hash

The ledger stores version and name but not a hash of the SQL body. Changing migration SQL without changing its version is therefore invisible to the runner.

### Stale migration documentation

Some comments still describe older version bands such as session version 100 and platform versions 101–199, while the current platform migrations use 201–203. This is a maintenance risk when adding future migrations.

## Foreign key findings

`elph-agent` applies:

```sql
PRAGMA foreign_keys = ON
```

`floppy` only applies `busy_timeout`. Because both can use the same `.elph/store.db`, foreign key enforcement depends on which layer created the connection. The session schema explicitly requires foreign keys to be enabled, so this behavior is inconsistent with the schema contract.

`elph-agent::connect` also logs and ignores pragma failures, meaning a connection may be returned without the intended configuration.

## Store-level concurrency findings

### Goals

`create_goal` checks for an unfinished goal and then inserts one in a separate operation. Two processes can both pass the check and insert active goals. There is no database uniqueness constraint or transaction-level conditional insert enforcing one active goal per session.

### Workers

Worker registration performs stale demotion, name selection, deletion, and insertion as separate operations. Two processes can select the same display name before either inserts it. There is no unique active-name constraint.

### Todos

`merge` reads the full list, merges in memory, and calls `replace`, which deletes and reinserts the complete list. Concurrent writers can overwrite each other's changes. WAL atomicity does not provide semantic merge behavior.

### ConnectionPool

`floppy::ConnectionPool` uses a semaphore only around the connection acquisition call. The permit is released before the returned `Connection` is used. It therefore limits concurrent connection attempts, not live connections or database operations. The implementation documents this, but the type name can imply stronger pooling guarantees.

## Floppy memory findings

`floppy` enables multiprocess WAL and, for some paths, experimental vacuum and index methods. Memory migrations use the shared `app_migrations` ledger and fall back from FTS migration based on error-message substrings such as `fts` or `index method`.

Risks include:

- sidecar cleanup before opening a database shared with the host;
- fallback classification based on broad error substrings;
- partial FTS migration state if a process stops during DDL;
- no validation that an injected `Database` was opened with required experimental index options;
- no visible maintenance lock protocol for in-place `VACUUM`.

Turso documents that in-place `VACUUM` requires no other process to hold the multiprocess WAL. The caller must therefore coordinate exclusive maintenance before invoking it.

## Platform and filesystem constraints

Turso multiprocess WAL requires supported 64-bit platforms, local filesystems, coherent mmap behavior, and POSIX byte-range locking. Turso explicitly rejects or does not validate filesystems such as NFS, SMB/CIFS, CephFS, GFS2, Lustre, OCFS2, AFS, CODA, NCP, and 9P/v9fs.

The project does not appear to perform platform or filesystem capability checks, warnings, or fallback policy before enabling multiprocess WAL.

## Severity summary

### P0

- Crash/reopen, checkpoint, schema-refresh, and vacuum-exclusion behavior is not covered by integration tests.
- Deployment on unsupported distributed filesystems is not rejected or diagnosed.
- Session open caches the tree; a process must reload after a `Conflict` rather than retrying the stale object.

### P1

- Explicit commit-outcome/idempotency reporting is still absent for unknown commit status.
- Goal and worker invariants should also have database-level uniqueness constraints.
- The Turso helpers remain duplicated across `elph-agent` and `floppy`.
- `VACUUM` still needs an exclusive maintenance protocol.

### P2

- Migration comments are stale in some historical sections.
- FTS fallback now matches only known capability errors from the Turso index method parser. Unrelated migration failures are returned instead of being hidden by broad message matching.
- Structured operation names, process identifiers, and metrics for retry exhaustion, rollback failures, migration waits, and session conflicts remain recommended observability work.

## P1 hardening status

The following P1 write paths use the shared serialized transaction policy:

- goal creation checks and inserts under one `BEGIN IMMEDIATE` transaction;
- worker registration and heartbeat use transactional writes;
- todo replace and clear use the shared transaction helper;
- floppy memory database paths use the same multiprocess WAL and vacuum builder options;
- session deletion and physical pruning are atomic with their related pointer/edge updates.

Session-tree optimistic leaf compare-and-swap and automatic index refresh remain required before concurrent writers may safely mutate the same session from independent processes. A failed CAS must surface as a conflict and trigger a reload rather than silently applying last-writer-wins.

Multiprocess WAL deployment still requires a local filesystem with coherent mmap and POSIX byte-range locking. Do not use NFS, SMB/CIFS, CephFS, or other unsupported distributed filesystems. Turso owns the `-wal` and `-tshm` sidecars; application code must not delete them.

## P2 hardening status

- Transaction rollback and commit failures are terminal and are never blindly replayed.
- Store writes use the shared serialized transaction helper instead of ad-hoc transaction handling.
- Turso sidecar lifecycle remains owned by Turso.
- Database configuration is aligned across `elph-agent` and `floppy` for multiprocess WAL, index support, vacuum support, busy timeout, and foreign keys.
- Retry classification remains limited to explicit lock/busy conditions; unrelated I/O and permission errors are not retryable.

Structured operation names, process identifiers, and metrics for retry exhaustion, rollback failures, migration waits, and session conflicts remain recommended follow-up observability work.

2. Build a two-process test harness before changing production code.
3. Define ownership rules for `.tshm`, `-wal`, and `-shm`; remove mtime-based liveness assumptions from the design.
4. Define the supported transaction model explicitly as serialized WAL writes, not MVCC.
5. Define session-level consistency semantics for concurrent writers and stale in-memory indexes.
6. Make startup migration behavior safe under concurrent process startup.
7. Unify `elph-agent` and `floppy` database configuration behavior.
8. Add separate tests for crash recovery, schema changes, checkpointing, vacuum exclusion, and cross-process snapshots.

## Sources

- [Turso Multi-Process Access](https://docs.turso.tech/sql-reference/multiprocess-access)
- [Turso Transactions](https://docs.turso.tech/sql-reference/statements/transactions)
- [Turso PRAGMA reference](https://docs.turso.tech/sql-reference/pragmas)
- [Turso Concurrent Writes](https://docs.turso.tech/tursodb/concurrent-writes)
- [Turso VACUUM](https://docs.turso.tech/sql-reference/statements/vacuum)
- [Turso PR #6236: Initial multiprocess database support](https://github.com/tursodatabase/turso/pull/6236)
- [Turso PR #7350: Rust SDK multiprocess WAL file lock](https://github.com/tursodatabase/turso/pull/7350)
- Turso changelog entries for multiprocess WAL locking, SDK races, stale checkpoint handling, and MVCC/multiprocess incompatibility.

## Validation performed

The report was produced by source inspection and upstream documentation research. No repository source files were changed for this report.

A focused existing test run was previously observed to pass:

```text
cargo test -p elph-agent datastore::conn::tests -- --nocapture
7 passed, 0 failed
```

That test run covers same-process helper behavior only. It does not validate true multiprocess operation.
