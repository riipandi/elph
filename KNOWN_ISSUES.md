# Known Issues

## 2026-07-04

- **Pi OS 32-bit (armv7)** — cross-compile fails; `turso`/`io-uring` does not support armv7. Use Pi OS **64-bit** → `*-linux-glibc-arm64.tar.gz`.
- **macOS** — no `cross-rs` Docker image; `*-macos-*` archives are produced only when `make cross` runs on a Mac.

Platform details: [docs/limitation.md](./docs/limitation.md).

## 2026-08-03 — Turso-native FTS: incompatible with standard SQLite tools

`store.db` uses Turso-native FTS indexes (`CREATE INDEX ... USING fts`, Tantivy-backed)
for memory (V4) and codegraph (V501). Turso stores internal metadata in
`__turso_internal_fts_dir_*` tables whose schema contains `USING` syntax — this is
valid in Turso but **not** recognised by standard SQLite.

Opening `store.db` with tools like TablePro, DB Browser for SQLite, or `sqlite3` CLI
will fail with:

```
malformed database schema (__turso_internal_fts_dir_idx_cg_chunks_fts_key) - near "USING": syntax error
```

**Impact:** read-only — the database is fully functional when opened by Turso (Elph
runtime). Third-party SQLite tools cannot open it.

**Workaround:** use a libSQL-compatible GUI like [Dataflare](https://github.com/DataflareApp/Dataflare)
instead of standard SQLite tools. Or use `elph` CLI commands (`elph memory list`,
`elph codegraph search`, etc.).

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

GPU device selection is handled at **compile time** via cargo features, not at runtime. The `models.embed.gpuAcceleration` setting controls whether GPU is attempted (on/off/auto) but cannot switch between CPU/GPU dynamically without rebuilding. To use GPU, you must rebuild with the appropriate feature flag.
