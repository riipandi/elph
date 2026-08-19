# Platform Limitations

Known release and platform constraints.

## Release archives (`make release`)

Linux builds are glibc only; musl/Alpine is not published yet. Windows is deferred.

| Archive                  | Target                      | For                             |
| ------------------------ | --------------------------- | ------------------------------- |
| `*-linux-x86_64.tar.gz`  | `x86_64-unknown-linux-gnu`  | RedHat/Ubuntu/Debian x86_64     |
| `*-linux-aarch64.tar.gz` | `aarch64-unknown-linux-gnu` | Pi 3/4/5 64-bit OS, ARM64 glibc |
| `*-macos-arm64.tar.gz`   | `aarch64-apple-darwin`      | macOS on Apple Silicon          |
| `*-macos-x86_64.tar.gz`  | `x86_64-apple-darwin`       | macOS on Intel                  |
| `*-win-*.zip`            | `*-pc-windows-*`            | Windows (deferred)              |

## Not supported

- **Pi OS 32-bit** (`armv7`) — Turso / io-uring constraint
- **Android, iOS**, and other mobile/embedded targets

## Turso-native FTS: incompatible with standard SQLite tools

`store.db` uses Turso-native FTS indexes (`CREATE INDEX ... USING fts`, Tantivy-backed)
for memory (V4). Turso stores internal metadata in
`__turso_internal_fts_dir_*` tables whose schema contains `USING` syntax — this is
valid in Turso but **not** recognised by standard SQLite.

Opening `store.db` with tools like TablePro, DB Browser for SQLite, or `sqlite3` CLI
will fail with:

```
malformed database schema (__turso_internal_fts_dir_idx_memories_fts_key) - near "USING": syntax error
```

**Impact:** read-only — the database is fully functional when opened by Turso (Elph
runtime). Third-party SQLite tools cannot open it.

**Workaround:** use a libSQL-compatible GUI like [Dataflare](https://github.com/DataflareApp/Dataflare)
instead of standard SQLite tools. Or use `elph` CLI commands (`elph memory list`, etc.) instead of external SQLite tools.

## Single-platform cross build

Use `make cross CROSS_TARGET=<triple>` (e.g. `aarch64-unknown-linux-musl`) to build one target at a time.
