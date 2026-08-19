# Platform Limitations

Known release and platform constraints.

## Release archives (`make release`)

Linux builds are glibc only; musl/Alpine is not published yet. Windows builds run on GitHub-hosted runners (no Namespace Windows runner yet).

| Archive                  | Target                      | For                             |
| ------------------------ | --------------------------- | ------------------------------- |
| `*-linux-x86_64.tar.gz`  | `x86_64-unknown-linux-gnu`  | RedHat/Ubuntu/Debian x86_64     |
| `*-linux-aarch64.tar.gz` | `aarch64-unknown-linux-gnu` | Pi 3/4/5 64-bit OS, ARM64 glibc |
| `*-macos-arm64.tar.gz`   | `aarch64-apple-darwin`      | macOS on Apple Silicon          |
| `*-macos-x86_64.tar.gz`  | `x86_64-apple-darwin`       | macOS on Intel                  |
| `*-windows-x86_64.zip`   | `x86_64-pc-windows-msvc`    | Windows x86_64 (GitHub-hosted runner) |

## Not supported

- **Pi OS 32-bit** (`armv7`) — Turso / io-uring constraint
- **Android, iOS**, and other mobile/embedded targets

## Windows (CI)

Windows binaries and tests run on GitHub-hosted `windows-latest` runners (no Namespace
Windows runner yet). Two Windows-only `make test` failures were historically present; both
are now resolved (2026-08-20):

- **Turso multiprocess WAL** — `experimental_multiprocess_wal(true)` is rejected by Turso's
  Windows IO backend (`experimental multiprocess WAL is not supported by the active IO
  backend`). The flag is now gated off on Windows; behavior on Unix is unchanged.
- **Abort race-test hang** — `concurrent_aborts_do_not_deadlock` blocked all tokio workers
  on low-CPU Windows runners, starving the abort tasks and the test timeout. The test now
  pins `worker_threads = 8`.

No known Windows limitation remains in the test suite.

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
