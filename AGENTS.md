# AGENTS.md

## Docs (`docs/`)

Update **before** marking task done when change is significant: new/changed public API, CLI cmd, config/env var, behavior a caller notices, error-contract change, new dependency, new/changed module interaction.

Skip for: pure internal refactor, lint/format, test-only, typos, dep bumps w/o API impact.

- Search `docs/` by feature/module name first — don't assume no doc exists.
- Found -> edit in place, touch only affected sections, keep tone/structure.
- Not found + warrants one -> create new doc, match folder's naming convention.
- Docs must reflect **current code**, not planned/old behavior — verify against the diff, not memory.
- Fix any now-stale statements in the same doc; remove refs to removed/renamed APIs.
- Ambiguous significance -> `ask_user_question`; if unavailable, state assumption + proceed.
- Scope = only what changed. Match existing Markdown conventions in that folder.

## Implementation

- Simplest impl for current requirements. No speculative abstraction/config/flags for hypothetical future needs.
- No backward compat unless explicitly requested — change/remove old APIs/schemas/behavior directly, no shims/dual paths.

## Cross-platform (macOS / Linux / Windows)

**Code**

- OS-specific APIs -> `#[cfg(target_os = "...")]`. Gate helper fns too, or `-D warnings` fails `dead_code` on platforms where unused.
- Binding must exist in all builds but only conditionally mutated -> use `cfg!(target_os = "...")` **macro**, not the attribute (avoids `unused_mut`/`E0596`).
- Paths: `Path`/`PathBuf`/`MAIN_SEPARATOR` only — never hardcode `/` or `\`.
- Unix-only facilities (PTY, signals, `fork`, `/proc`, `which`) -> `#[cfg(unix)]` + Windows path.
- Storage backends rejecting Windows multiprocess WAL -> `cfg!(not(target_os = "windows"))`.

**Lint** (`make lint` = clippy `-D warnings`, all platforms)

- No `return` in tail position of `#[cfg(windows)]` (or other cfg) blocks — use tail expression (`needless_return`).
- Ungated OS-only helper -> `dead_code` fails on the platforms that don't call it.

**Tests**

- No POSIX shell assumption. Windows: `elph-agent` shell exec -> git-bash, `$PWD` is MSYS format (`/c/Users/...`), not `C:/Users/...`. Probe via `cygpath -m "$PWD"` or make assertion platform-aware.
- Prefer `std::fs`/`Path` assertions over parsing shell output.
- Genuinely non-portable test -> `#[cfg(not(target_os = "windows"))]` (or relevant OS) + comment explaining why.

## Rust imports

- Don't mix types (struct/enum/type alias) and functions in one braced `use` if it wraps.
- Separate `use` lines > one long braced import wrapping awkwardly.
- Group by kind: types / functions / traits.
- Trailing comma on last item of multi-item braces.
- One line per group if it fits 120 width; otherwise split into multiple `use` statements.
- `cargo fmt` is authoritative for final layout — write to match this before formatting.

## Build system

Always `make`, never direct `cargo` (bypasses sccache):

| Task          | Target                                  |
| ------------- | --------------------------------------- |
| Format        | `make fmt`                              |
| Check         | `make check` (`-- -p <pkg>` for subset) |
| Lint          | `make lint` (`-- -p <pkg>` for subset)  |
| Test          | `make test` (`-- -p <pkg>` for subset)  |
| Build         | `make build` / `make install`           |
| Stats         | `make stats`                            |
| Space reclaim | `make gc` (`DRY=1` to preview)          |

`make` sets `RUSTC_WRAPPER=sccache`, `SCCACHE_DIRECT=true`, `SCCACHE_MAXSIZE=20G`.
No matching target -> `RUSTC_WRAPPER=sccache SCCACHE_DIRECT=1 cargo <cmd>`.

sccache notes: cold cache on first run is normal, remote bucket (`r2-sccache`) warms after one full build (expect 40%+ hit rate after). Incremental dirs show `Non-cacheable` — expected; `make gc`/`make clean-incremental` safe. `SCCACHE_DISABLE=1` only when debugging sccache itself.

## Testing

**Unit** — same file or `src/<module>/tests.rs`, `#[cfg(test)]`, may test `pub(crate)` directly.

**Integration** (`tests/`, separate crate) — public contract only: crate-root prelude + documented modules (`harness`, `session`, `mcp`, `compaction`, ...). Don't `pub`/flatten an item just so `tests/` compiles — that's a unit test, move it next to the impl.

**`elph-ai` / `elph-agent` surface** — crate root = prelude (`Agent`, `Models`, constructors, typed errors). Domain APIs stay on `pub mod` (`elph_agent::compaction::should_compact`, not flattened). No `test-utils`/`integration-test` feature to leak internals. `#[doc(hidden)]` only for existing fixtures that must stay `pub` (e.g. process-wide MCP key helpers).

**General** — small, focused, cover edge/failure paths, no dup between unit/integration, descriptive names.
