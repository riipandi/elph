# Known Issues

## 2026-07-04

- **Pi OS 32-bit (armv7)** — cross-compile fails; `turso`/`io-uring` does not support armv7. Use Pi OS **64-bit** → `*-linux-glibc-arm64.tar.gz`.
- **macOS** — no `cross-rs` Docker image; `*-macos-*` archives are produced only when `make cross` runs on a Mac.

Platform details: [docs/limitation.md](./docs/limitation.md).

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
