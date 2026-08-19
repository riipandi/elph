# Known Issues

## 2026-08-20 — Windows CI: Turso multiprocess WAL + low-CPU abort-test hang (resolved)

Two Windows-only `make test` failures were fixed:

1. **Turso `experimental_multiprocess_wal` unsupported on the Windows IO backend.** Every
   local Turso database open (`floppy`, `coding-agent` datastore/transcript cache,
   `elph-agent` datastore) used `experimental_multiprocess_wal(true)`. Turso's Windows IO
   backend rejects this with `experimental multiprocess WAL is not supported by the active
   IO backend`, so all turso-backed tests failed to open `store.db` on Windows.

   Fixed by gating the flag off on Windows at every open site via
   `experimental_multiprocess_wal(cfg!(not(target_os = "windows")))`. Behavior on Unix is
   unchanged (`cfg!` evaluates to `true`). `experimental_index_method(true)` is kept — it
   works on Windows.

2. **`concurrent_aborts_do_not_deadlock` hung on low-CPU Windows runners.** The test runs 4
   harnesses whose provider factory blocks a tokio worker (`std::thread::sleep`) to simulate
   a stuck request. With the default `#[tokio::test(flavor = "multi_thread")]` worker count
   (= CPU count), a 2-vCPU Windows runner let those 4 blocking turns starve the abort tasks
   and the test's own 5s timeout, so the test hung until the 1200s job timeout.

   Fixed by pinning `worker_threads = 8` on that test so abort tasks stay schedulable.
   Test-only change; Unix behavior unchanged.

### Status

Resolved 2026-08-20. Affected files: `crates/floppy/src/core/db.rs`,
`crates/floppy/src/memory/migrations.rs`, `crates/elph-agent/src/datastore/conn.rs`,
`crates/coding-agent/src/platform/datastore/mod.rs`,
`crates/coding-agent/src/tui/transcript/cache.rs`, `crates/coding-agent/tests/unified_store.rs`,
`crates/elph-agent/tests/harness.rs`.

## 2026-08-18 — Intel MKL embeddings cannot link with the wild linker

`.cargo/config.toml` uses [wild](https://github.com/wild-linker/wild) for `x86_64-unknown-linux-gnu` (`clang --ld-path=wild`). `floppy` embeddings go through `embed_anything` / Candle.

Enabling `embed_anything/mkl` (Intel MKL via `intel-mkl-src`) fails at link time with wild:

```
wild: error: Undefined symbol hgemm_, referenced by candle-core …/mkl.rs
```

`hgemm_` is an MKL BLAS symbol. Wild does not resolve the static MKL archives the way GNU ld / lld do.

### Current behavior

- **macOS** — Apple Accelerate (`embed_anything/accelerate`). Unrelated to this issue.
- **Linux / Windows (default)** — Candle’s built-in CPU backend. No MKL.
- **Opt-in MKL** — `floppy` feature `mkl` (`embed_anything/mkl`). Use only with GNU ld or lld, not wild.

`floppy/full` and the `elph` binary do **not** enable `mkl`. CI Linux AMD64 therefore links with wild.

### Workaround

Keep the default CPU backend, or disable wild for that target and build with MKL:

```toml
# crates/coding-agent or a local override — not compatible with wild
floppy = { workspace = true, features = ["full", "mkl"] }
```

```bash
# drop wild for one build, then:
# RUSTFLAGS='-C link-arg=-fuse-ld=lld' make test
```

### Follow-up

Re-test `floppy/mkl` when wild can link MKL static archives, or document a supported linker matrix if MKL becomes a default again.

## 2026-08-13 — Turso multiprocess WAL integration requires hardening

The project uses Turso `0.8.0-pre.4` with `experimental_multiprocess_wal(true)` for `.elph/store.db` on Unix. On Windows the flag is disabled (`experimental_multiprocess_wal(cfg!(not(target_os = "windows")))`) because Turso's Windows IO backend does not support multiprocess WAL. The builder configuration is consistent, but the surrounding integration is not yet production-ready for concurrent OS processes.

### High-risk findings

- `TranscriptCache` no longer checkpoints/truncates WAL on every open. Checkpointing remains Turso-managed for multiprocess access.
- WAL recovery can delete a short `-wal` file while another process may still be initializing or writing it. The generic `unable to open database file` error is also too broad to justify sidecar deletion.
- No true two-process integration test currently proves that separate Rust processes can open, write, checkpoint, and recover the same database file.
- `with_mvcc_transaction` uses serialized `BEGIN IMMEDIATE`; it is not Turso MVCC. Turso MVCC requires `journal_mode = mvcc` and `BEGIN CONCURRENT`, and MVCC is incompatible with multiprocess WAL.
- Lock retry does not cover every transaction phase, especially `BEGIN IMMEDIATE` and `COMMIT`.
- Session indexes are cached in memory. A process that keeps a session open can miss entries written by another process, and `active_leaf_id` has last-writer-wins behavior without an expected-old-leaf check.
- Migration existence checks, DDL, and migration-ledger writes are not one atomic operation. Concurrent startup and destructive schema rebuilds can leave partial or conflicting migration state.
- Goal, worker-name, and todo read-modify-write flows have TOCTOU or lost-update risks under concurrent processes.
- `elph-agent` and `floppy` duplicate Turso open/configuration helpers with different behavior. In particular, foreign-key enforcement is enabled in `elph-agent` but not consistently in `floppy`.

### Operational limitations

- Turso multiprocess WAL is experimental. Its `.tshm` format and public API may change between releases.
- The database must use a supported 64-bit platform and a local filesystem with coherent mmap and POSIX byte-range locking. Network/distributed filesystems such as NFS, SMB/CIFS, CephFS, Lustre, and similar filesystems are not safe.
- In-place `VACUUM` requires that no other process hold the multiprocess WAL. The project does not currently document or enforce a dedicated maintenance lock protocol.
- The dependency is a pre-release. Upstream Turso changelogs record recent fixes for multiprocess WAL file locks, SDK races, stale checkpoint state, and WAL recovery behavior. The exact contents of the pinned version must be verified before relying on cross-process access.

### Current workaround

Use one long-lived `Arc<Database>` per process and avoid opening the same session from multiple processes when possible. Do not manually remove `.tshm`, `-shm`, or `-wal` files while any Elph process may still be using the database. Use the detailed audit for scope, evidence, and remediation order:

- [docs/turso-multiprocess-wal-audit.md](./docs/turso-multiprocess-wal-audit.md)

### Required follow-up

1. Verify the pinned Turso version includes the upstream multiprocess WAL Rust SDK lock fixes.
2. Add a two-process test harness for open, concurrent writes, snapshots, checkpointing, and crash/reopen recovery.
3. Replace mtime-based sidecar liveness and automatic deletion with an ownership-safe recovery policy.
4. Make migration startup and session-level read-modify-write behavior safe under concurrent processes.

## 2026-07-04

- **Pi OS 32-bit (armv7)** — cross-compile fails; `turso`/`io-uring` does not support armv7. Use Pi OS **64-bit** on aarch64 hardware; arm64 Linux binaries are not published (CI builds x86_64 Linux only), so build from source there.
- **macOS** — `*-macos-*` archives are built in CI via native `cargo build` for both x86_64 and arm64; no `cross-rs` Docker image is needed.

Platform details: [docs/limitation.md](./docs/limitation.md).

## 2026-08-03 — Turso-native FTS: incompatible with standard SQLite tools

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
instead of standard SQLite tools. Or use `elph` CLI commands (`elph memory list`, etc.).

## 2026-07-26 — Vendor iocraft: OSC 8 Hyperlinks

Elph uses `vendor/iocraft/` (iocraft v0.8.4 with patches) because OSC 8 hyperlink
support has not been released on crates.io yet (PR [#216](https://github.com/ccbrown/iocraft/pull/216)
is still open).

### Changes from upstream

All changes are **additive** — no existing APIs are modified:

| File                | Additions                                                                                                                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/canvas.rs`     | `Character.hyperlink`, `CanvasCell.hyperlink`, `set_hyperlink()`, `set_text_with_hyperlink()`, `hyperlink_at()`, `hyperlink_index()`, OSC 8 rendering (`\x1b]8;;url\x1b\\`) in `write_row_impl()` |
| `src/mixed_text.rs` | `MixedTextContent.hyperlink` + `.hyperlink(url)` builder, propagation to `TextDrawer.append_lines_with_hyperlink()`                                                                               |
| `src/text.rs`       | `TextDrawer.append_lines_with_hyperlink()`                                                                                                                                                        |
| `src/strip_ansi.rs` | `sanitize_terminal_text()` (more comprehensive than `strip_ansi`), `sanitize_osc8_uri()` (terminal injection safety)                                                                              |

### Limitations

- **OSC 8 escape sequences are always emitted** to the terminal every frame — text with
  hyperlinks still renders as links in supporting terminals.
- **Clicking** only works when **mouse capture is OFF** (select text mode active,
  `set_mouse_capture(false)`). In normal mode (mouse capture ON), clicks are intercepted
  by the application and the terminal never receives the event to activate the hyperlink.
- The terminal must support OSC 8 (iTerm2, Kitty, WezTerm, Alacritty, Windows Terminal,
  etc.) and the user needs to **Cmd+Click** (macOS) or **Ctrl+Click** (Linux/Windows).

## GPU Support: Available via compile-time features

GPU acceleration for embeddings is available via cargo features. candle-kernels 0.11.0 is available on crates.io.

### Platform Support

- **macOS ARM64** (Apple Silicon M1/M2/M3/M4): Uses Apple Metal via `metal` feature
- **Linux/Windows with NVIDIA GPU**: Uses CUDA via `cuda` feature
- **Auto-detection**: `GpuConfig::detect()` checks OS and hardware availability at runtime

### To Enable GPU

Add GPU feature to your build:

```bash
# For macOS ARM64 (Apple Silicon)
cargo build --features metal

# For Linux/Windows with NVIDIA GPU
cargo build --features cuda
```

Or enable in `crates/coding-agent/Cargo.toml` dependency on floppy:

```toml
floppy = { path = "../crates/floppy", version = "0.0.1", features = ["full", "metal"] }
# or
floppy = { path = "../crates/floppy", version = "0.0.1", features = ["full", "cuda"] }
```

### Settings Configuration

Configure GPU acceleration mode in `settings.json`:

```json
{
    "models": {
        "embed": {
            "gpuAcceleration": "auto"
        }
    }
}
```

Options:

- `"auto"` (default): Auto-detect and use GPU if available
- `"on"`: Always use GPU (fails if GPU unavailable or feature not enabled)
- `"off"`: Never use GPU (CPU-only)

### Current Limitation

GPU device selection is handled at **compile time** via cargo features, not at runtime. The `models.embedGpuAcceleration` setting controls whether GPU is attempted (on/off/auto) but cannot switch between CPU/GPU dynamically without rebuilding. To use GPU, you must rebuild with the appropriate feature flag.
