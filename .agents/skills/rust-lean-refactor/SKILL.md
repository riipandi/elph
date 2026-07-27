---
name: rust-lean-refactor
description: >-
    Reorganize Rust code to be lean, clean, and non-bloated: detect oversized files,
    functions, or modules and split them safely across crate/module boundaries without
    changing behavior. Guards against new circular mod dependencies, skips codegen/generated
    files, and verifies cargo check/clippy/test stay green at every step. Use when user asks
    to clean up, declutter, slim down, de-bloat, restructure, or split up Rust code, or
    complains a file/module/function is "gemuk"/too big/doing too much. Pair with
    rust-verify-harden for the hardening pass after structure is settled.
---

# Rust Lean Refactor

## Language

Split by destination:

- **In-chat responses, plans, summaries** — follow the language the user is currently using (Indonesian, English, etc). Code, paths, identifiers, crate/lint names stay literal English.
- **Any documentation edits written to files** (`docs/**`, permanent code comments) — **always English**, regardless of chat language.

## Purpose

Make Rust code lean and readable through pure structural refactoring — splitting bloated files/functions/modules, deduping, tightening naming — while never changing observable behavior and never leaving a gate red. Split is not delete: removing actual dead or legacy code follows a stricter, separate rule (Phase 2 / Phase 5).

**Relationship to `rust-verify-harden`:** this skill owns _structure_ (what lives where, how big things are); `rust-verify-harden` owns _hardening_ (memory/deadlock/race/perf, quality gates). Don't duplicate the concurrency/memory audit here — if the user also wants that, run `rust-verify-harden` after this skill's structure settles, not interleaved with it.

## Scope

- Default scope: the crate(s)/module(s) the user names, or the crate(s) touched by their last few messages. Do **not** silently expand to the whole workspace unless asked.
- **Intra-module split** (functions/types moved within the same crate) is the default operating mode — low risk, no `Cargo.toml` changes.
- **Cross-crate split** (moving code so it now lives in a different crate, e.g. `elph-agent` → `elph-ai`) is a bigger change: it touches `Cargo.toml` deps, may create new pub surface, and risks a crate-level circular dependency. Flag this explicitly as its own step, propose it, and get a go-ahead before executing — don't fold it into routine intra-crate splitting.

## Safety Rules

- **No behavior change.** Pure structural refactor: move, split, rename, dedupe, reorder. If a "clean" idea changes logic, output, or a public API contract, flag it separately instead of doing it here.
- **Gates green at every stopping point:** `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` / `cargo nextest run` (or `make check/lint/test` if the Makefile wraps these). Never leave one red between steps.
- **No new circular mod dependencies.** After each split, confirm the new module graph is still acyclic — see Phase 4.3.
- **Codegen/generated files are out of scope.** Skip anything under a `generated/`, `*.gen.rs`, `build.rs`-produced `OUT_DIR` output, `bindings.rs`, or files carrying a `// @generated` / `#[automatically_derived]`-style header. Splitting these fights the codegen tool on the next run.
- **Small, verifiable increments** — one bloated unit at a time, re-verify before the next.
- **Legacy/back-compat code is out of scope for silent deletion** — `#[deprecated]`, `_v1`/`_old`/`_legacy` suffixes, compat shims, retired `#[cfg(feature = ...)]` paths. Splitting these out into their own module is fine; deleting them needs the same ask-before-remove checkpoint as `rust-verify-harden` Phase 4.
- **Respect existing module conventions** — mirror how the crate already organizes `mod.rs`/file-per-module/`pub(crate)` boundaries rather than importing a generic style.
- **No unprompted new deps/tools.** Use what's already available (`clippy`, `tokei`, `cargo-modules`, `rg`); if a tool isn't present, fall back to `rg`/manual reading rather than installing something new.

## Workflow

### Phase 1 — Baseline

1. Confirm scope (crate/module) with the user's request; don't assume workspace-wide unless stated.
2. Run baseline gates once, scoped to the relevant package(s):
    ```sh
    cargo check -p <crate>
    cargo clippy -p <crate> --all-targets -- -D warnings
    cargo test -p <crate>
    ```
    Note pre-existing failures — not this skill's job to fix unrelated red gates; call them out instead.
3. `git status -sb` — warn if the tree is dirty before starting.
4. Size baseline for the target scope:
    ```sh
    tokei <path> --sort lines            # per-file line counts, if tokei is available
    rg -c "^fn |^    fn |^pub fn " <path> # rough function-count signal as fallback
    ```

### Phase 2 — Bloat scan (identify only, don't touch yet)

Concrete thresholds (adjust for genuinely dense but simple files — e.g. big match-based dispatch tables — rather than applying blindly):

- **File**: flag if noticeably larger than sibling files in the same module tree, and as a rough floor, > ~400 lines for non-generated, non-match-table files.
- **Function**: flag if > ~60 lines, or > ~5 parameters, or nesting depth > ~3, or clearly doing more than one job (parse + validate + persist in one fn, etc).
- **Module**: flag "god modules" — a single file/`mod` accumulating unrelated `pub fn`s, or a `utils.rs`/`helpers.rs` that has become a dumping ground.
- **Duplication**: near-identical blocks across files — use `rg` or `ast-grep` pattern search, not eyeballing.
- **Dead code**: confirm via `cargo clippy` `dead_code`/`unused` lints or `cargo +nightly udeps` if present — never by eye.
- **Legacy/back-compat markers**: `#[deprecated]`, `_v1`/`_old`/`_legacy` suffixes, compat shims, retired feature flags — flag same as dead code, but removal needs Phase 5 approval.
- **Skip list**: exclude anything matching the codegen/generated patterns in Safety Rules before finalizing the list.

Output a prioritized list: path → signal → why it's bloated → proposed split boundary (module/function it becomes) → intra-crate or cross-crate.

### Phase 3 — Plan the splits

1. For each flagged unit, name the extraction target (new `mod` name/file path), what moves, what stays, and the seam (which fns/types/traits become the new boundary — often the natural `pub(crate)` cut point).
2. Order leaf-first: extract pieces with no cross-dependencies before ones that depend on an earlier split's result.
3. Any cross-crate split gets called out explicitly with the `Cargo.toml` diff it implies (new dep edge, new pub export) — propose it as a distinct step, not folded into the intra-crate batch.
4. If a split would leak private state, break encapsulation, or force an ownership/lifetime redesign, don't force it — surface it as a design question instead.
5. For a large plan, give the user a short numbered summary before executing, unless they've already said to just proceed.

### Phase 4 — Execute (one unit at a time)

1. Extract the cohesive piece into its new file/`mod`, matching the crate's existing module-declaration style (`mod.rs` vs file-per-module, `pub`/`pub(crate)` visibility already in use nearby).
2. Update every `use`/`mod` reference and re-export (`rg "use .*<old_path>"` across the workspace, not just the crate — other crates may import it).
3. **Mod-cycle check**: after wiring the new module in, confirm no new cycle was introduced.
    ```sh
    cargo modules generate graph -p <crate> 2>/dev/null | grep -i cycle
    # if cargo-modules isn't installed, cargo check itself will hard-error on most
    # genuine mod cycles — treat a fresh compile error here as a signal, not noise
    ```
4. Re-run the scoped gate suite (`cargo check`/`clippy`/`test` for the touched package(s), plus any dependent crate if the split was cross-crate). Must be green before the next unit.
5. If a gate breaks: fix immediately, or roll back just this split — commit each split locally as you go so rollback is exact:
    ```sh
    git add -A && git commit -m "refactor: split <old> -> <new> (intra-crate)"
    # bad split -> git reset --hard HEAD~1   (only this split, not the whole session)
    ```
    Don't leave two unverified splits stacked at once.
6. Keep diffs mechanical — no "while I'm here" logic tweaks or opportunistic renames-that-change-behavior. File those as separate suggestions.

### Phase 5 — Clean pass

1. Tidy naming across the new boundaries, collapse redundant re-exports, tidy `mod.rs`/`lib.rs` module trees.
2. Remove genuinely dead code found in Phase 2 — only code clippy/tooling confirmed dead, and only if it isn't a legacy/back-compat marker.
3. For anything flagged legacy/back-compat: ask the user per-item or as a batch — **remove** or **keep** — before touching it. Use `ask_user_input_v0` for a simple choice; a plain question if it needs more context. Don't infer "remove" from the original "clean this up" ask — that authorized structure work, not compat-code deletion.
4. Re-run full scoped gates after any removal.

### Phase 6 — Final gate & summary

1. Full gate suite green (scoped `cargo check`/`clippy -D warnings`/`test`, or `make check/lint/test` if that's the project's real wrapper) is the actual completion criterion.
2. Re-measure size for changed paths (`tokei <path> --sort lines`) to report a real before/after, not an impression.
3. Summarize:
    - **Splits made**: before path → after path(s), intra-crate or cross-crate, one-line reason.
    - **Size before → after** per unit (line counts from `tokei`/`wc -l`).
    - **Mod-cycle check**: confirmed clean, or what was restructured to avoid one.
    - **Dead code removed**: what, how confirmed dead.
    - **Legacy/back-compat**: what was found, what the user decided, what was actually removed/kept.
    - **Gate status**: final pass/fail per gate, per affected crate.
    - **Deferred items**: design questions or cross-crate proposals not yet actioned.
    - **Suggested next step**: if concurrency/memory/perf wasn't in scope here, note that `rust-verify-harden` is the natural follow-up now that structure is settled.

## Notes

- Splitting preserves all behavior; only Phase 5 touches removal, and only for provably dead code or explicitly approved legacy cleanup.
- If a session must pause mid-reorg, only pause at a green, committed checkpoint — never hand back a red gate or an uncommitted half-split as "in progress."
- A "lean" result that ignores the crate's existing module conventions isn't actually lean for that codebase.
- Cross-crate moves and legacy-code deletion are the two things this skill never does silently — both need an explicit go-ahead.
- Git: commit per split locally for rollback precision; push only if explicitly instructed.

## Example

User: "tidy up the Elph-Agent crate, if there is a fat module, just split it, make sure the gate is green"

Agent workflow:

1. Scope to `elph-agent`; baseline `cargo check/clippy/test -p elph-agent`, `tokei` size baseline, git status check.
2. Bloat scan (Phase 2) — flag oversized files/fns, duplication, dead code, legacy markers; exclude any generated files.
3. Plan splits leaf-first (Phase 3); call out any split that would cross into `elph-ai` as its own proposal.
4. Execute one split at a time, mod-cycle check + scoped gate re-run + local commit after each (Phase 4).
5. Clean pass — dedupe/rename/tidy; ask before removing anything legacy/back-compat (Phase 5).
6. Final gate run + summary, with a note that `rust-verify-harden` is the natural next pass for hardening.
