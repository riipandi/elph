# Cohesive Hardening Review — Turso Integration

## Review result

The hardening pass was re-verified after implementation. The project uses `turso 0.8.0-pre.4`, local `Builder::new_local`, experimental multiprocess WAL, and serialized `BEGIN IMMEDIATE` write transactions.

Quality gates passed:

```text
make check    passed
make lint     passed
make test     2651 tests passed, 13 skipped
```

## Cohesion fixes applied

### Transaction API consistency

All changed callers now use `with_write_transaction`. The helper name matches the actual model: multiprocess WAL serializes writers; the project does not enable Turso MVCC or `BEGIN CONCURRENT`.

### Sidecar ownership

The application no longer deletes or repairs `-wal`, `-shm`, or `-tshm` files. Turso owns these files and their coordination state.

### Connection configuration

`elph-agent` and `floppy` apply the same mandatory connection pragmas:

```sql
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
```

### Transcript cache

The `TranscriptCache` (SQLite snapshot store) and the legacy tree-append snapshot path were removed — both were dead code with no callers. Transcript persistence now relies solely on the session tree; retention sheds derived markdown caches and tool diff text in-memory instead of writing snapshots.

## Remaining known design risks

The following are not hidden by the hardening pass:

- There is still no true two-OS-process integration test.
- Session indexes are cached and can become stale when another process writes the same session.
- `active_leaf_id` remains last-writer-wins.
- Goal, worker-name, and todo read-modify-write workflows need database-level conditional writes or constraints.
- In-place `VACUUM` still requires a separate exclusive maintenance protocol.
- The Turso dependency remains a pre-release.

These items require explicit product-level concurrency semantics rather than another local retry loop.
