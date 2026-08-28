---
name: rust-lean-refactor
description: >-
    Reorganize Elph Rust code to be lean, clean, and non-bloated: detect oversized
    files, functions, or modules and split them safely across existing crate/module
    boundaries without changing behavior. Guards against new circular dependencies,
    public API drift, feature/target regressions, and generated-file churn, then verifies
    the repository's make check/lint/test gates. Use when user asks
    to clean up, declutter, slim down, de-bloat, restructure, or split up Rust code, or
    complains a file/module/function is "gemuk"/too big/doing too much. Pair with
    rust-verify-harden for the hardening pass after structure is settled.
metadata:
    scope: project
---

# Rust Lean Refactor

## Language

Split by destination:

- **In-chat responses, plans, summaries** — follow the language the user is currently using (Indonesian, English, etc). Code, paths, identifiers, crate/lint names stay literal English.
- **Any documentation edits written to files** (`docs/**`, permanent code comments) — **always English**, regardless of chat language.

## Purpose

Make Rust code lean and readable through pure structural refactoring — splitting
bloated files/functions/modules, isolating cohesive responsibilities, and
deduplicating genuinely identical private logic — while preserving observable
behavior, public API, error contracts, feature behavior, and platform behavior.
Split is not delete: removing actual dead or legacy code follows a stricter,
separate rule.

**Relationship to `rust-verify-harden`:** this skill owns _structure_ (what lives where, how big things are); `rust-verify-harden` owns _hardening_ (memory/deadlock/race/perf, quality gates). Don't duplicate the concurrency/memory audit here — if the user also wants that, run `rust-verify-harden` after this skill's structure settles, not interleaved with it.

### Elph architecture baseline

Confirm the dependency direction before choosing a boundary:

```text
elph (coding-agent app, CLI, TUI)
  ├── elph-agent (agent orchestration, sessions, tools)
  │     └── elph-ai (LLM types, catalogs, provider adapters)
  ├── elph-tui (reusable iocraft components)
  ├── floppy (memory/storage)
  └── rendown (terminal/Markdown rendering)
```

`elph-ai` and `elph-agent` are publishable libraries. Their crate roots are
preludes; domain APIs remain under documented modules. Do not flatten a module
or make an item public solely to make an integration test compile. Unit tests
for `pub(crate)` behavior belong beside the implementation.

## Scope

- Default scope: the crate(s)/module(s) the user names, or the crate(s) touched by their last few messages. Do **not** silently expand to the whole workspace unless asked.
- **Intra-module split** (functions/types moved within the same crate) is the default operating mode — low risk, no `Cargo.toml` changes.
- **Cross-crate split** (moving code so it now lives in a different crate,
  e.g. `elph-agent` → `elph-ai`) is a bigger change: it touches
  `Cargo.toml`, may create new public surface, changes publish packaging, and
  risks a crate-level circular dependency. Flag it explicitly, propose the
  dependency/API diff, and get a go-ahead before executing.

## Safety Rules

- **No behavior change.** Pure structural refactor: move, split, rename, dedupe, reorder. If a "clean" idea changes logic, output, or a public API contract, flag it separately instead of doing it here.
- **Use repository wrappers for gates:** `make fmt`, `make check`, `make lint`,
  and `make test`. Forward package selectors after `--` (for example
  `make check -- -p elph-agent`). Use `SCCACHE_DISABLE=1` only when the cache
  is unavailable and record it in the final report.
- **No new circular mod dependencies.** After each split, confirm the new module graph is still acyclic — see Phase 4.3.
- **Codegen/generated files are out of scope.** Skip anything under a
  `generated/`, `*.gen.rs`, `build.rs`-produced `OUT_DIR` output,
  `bindings.rs`, `vendor/iocraft/`, `crates/elph-ai/models/**/*.json`,
  `schemas/`, or files carrying a generated header. Use the owning generator
  or `make generate-models` instead of editing generated output.
- **Small, verifiable increments** — one bloated unit at a time, re-verify before the next.
- **Legacy/back-compat code is out of scope for silent deletion** — `#[deprecated]`, `_v1`/`_old`/`_legacy` suffixes, compat shims, retired `#[cfg(feature = ...)]` paths. Splitting these out into their own module is fine; deleting them needs the same ask-before-remove checkpoint as `rust-verify-harden` Phase 4.
- **Respect existing module conventions** — mirror how the crate already organizes `mod.rs`/file-per-module/`pub(crate)` boundaries rather than importing a generic style.
- **No unprompted new deps/tools.** Use what's already available (`clippy`,
  `tokei`, `cargo-modules`, `rg`, `ast-grep`). If a tool is absent, fall back
  to `wc -l`, `rg`, `cargo metadata`, and manual inspection rather than
  installing it.
- **Do not destroy parallel work.** Never use `git reset --hard` or broad
  `git checkout --` rollback commands. Preserve unrelated edits and ask when
  file ownership is unclear.
- **Git is opt-in:** do not commit or push unless explicitly instructed.
- **Cross-platform is part of behavior:** inspect `#[cfg]` branches and
  target-specific dependencies; do not move Unix-only helpers into code that
  must compile on Windows.

## Workflow

### Phase 1 — Baseline

1. Confirm scope (crate/module) with the user's request; don't assume
   workspace-wide unless stated.
2. `git status --short` and `git diff --stat`; identify external or unrelated
   edits before planning.
3. Run baseline gates through `make` for the relevant package:
   `make check -- -p <crate>`, `make lint -- -p <crate>`, and
   `make test -- -p <crate>`. Run full workspace gates when a public or
   cross-crate boundary is involved.
4. Record package MSRV, edition, enabled default features, target-specific
   dependencies, and whether the crate is published.
5. Size baseline for the target scope:

    ```sh
    tokei <path> --sort lines            # per-file line counts, if tokei is available
    rg -c "^fn |^    fn |^pub fn " <path> # rough function-count signal as fallback
    ```

### Phase 2 — Bloat scan (identify only, don't touch yet)

Concrete thresholds (adjust for genuinely dense but simple files — e.g. big match-based dispatch tables — rather than applying blindly):

- **File**: flag if noticeably larger than sibling files in the same module tree, and as a rough floor, > ~400 lines for non-generated, non-match-table files.
- **Function**: flag if > ~60 lines, or > ~5 parameters, or nesting depth
  > ~3, or clearly doing more than one job. These are investigation signals,
  not automatic extraction rules; parser state machines, provider payload
  builders, and large rendering match arms may be intentionally cohesive.
- **Module**: flag "god modules" — a single file/`mod` accumulating unrelated `pub fn`s, or a `utils.rs`/`helpers.rs` that has become a dumping ground.
- **Duplication**: near-identical blocks across files — use `rg` or `ast-grep` pattern search, not eyeballing.
- **Dead code**: confirm via Clippy/compiler lints or `cargo-udeps` if already
  installed — never by eye. Check feature-gated, target-gated, examples,
  integration tests, build scripts, and public exports before removing.
- **Legacy/back-compat markers**: `#[deprecated]`, `_v1`/`_old`/`_legacy` suffixes, compat shims, retired feature flags — flag same as dead code, but removal needs Phase 5 approval.
- **Skip list**: exclude anything matching the codegen/generated patterns in Safety Rules before finalizing the list.

Output a prioritized list: path → signal → why it's bloated → proposed split boundary (module/function it becomes) → intra-crate or cross-crate.

### Phase 3 — Plan the splits

1. For each flagged unit, name the extraction target (new `mod` name/file
   path), what moves, what stays, and the seam (which fns/types/traits become
   the new boundary — often the natural `pub(crate)` cut point).
2. Order leaf-first: extract pieces with no cross-dependencies before ones that depend on an earlier split's result.
3. Any cross-crate split gets called out explicitly with the `Cargo.toml` diff it implies (new dep edge, new pub export) — propose it as a distinct step, not folded into the intra-crate batch.
4. If a split would leak private state, break encapsulation, force an
   ownership/lifetime redesign, alter a public crate-root prelude, or change
   documented module paths, don't force it — surface it as a design question.
5. Check whether the change warrants documentation under the repository's
   `AGENTS.md` rules: public API, module interaction, CLI behavior, or config
   changes require docs updates.
6. For a large plan, give the user a short numbered summary before executing,
   unless they've already said to just proceed.

### Phase 4 — Execute (one unit at a time)

1. Extract the cohesive piece into its new file/`mod`, matching the crate's existing module-declaration style (`mod.rs` vs file-per-module, `pub`/`pub(crate)` visibility already in use nearby).
2. Update every `use`/`mod` reference and re-export (`rg "use .*<old_path>"` across the workspace, not just the crate — other crates may import it).
3. **Mod-cycle check**: after wiring the new module in, confirm no new cycle was introduced.

    ```sh
    cargo modules generate graph -p <crate> 2>/dev/null | grep -i cycle
    # if cargo-modules isn't installed, cargo check itself will hard-error on most
    # genuine mod cycles — treat a fresh compile error here as a signal, not noise
    ```

 1. Re-run the scoped gate suite with `make check`, `make lint`, and
    `make test`; include dependent crates if the split is cross-crate. Must be
    green before the next unit.
 2. If a gate breaks, fix immediately or revert only this split using a targeted
    patch. Do not commit automatically and do not use destructive rollback
    commands. Don't leave two unverified splits stacked at once.
4. Keep diffs mechanical — no "while I'm here" logic tweaks or opportunistic renames-that-change-behavior. File those as separate suggestions.

### Phase 5 — Clean pass

1. Tidy naming across the new boundaries, collapse redundant re-exports, and
   tidy `mod.rs`/`lib.rs` module trees without changing public paths.
2. Remove genuinely dead code found in Phase 2 — only code clippy/tooling confirmed dead, and only if it isn't a legacy/back-compat marker.
3. For anything flagged legacy/back-compat: ask the user per-item or as a batch — **remove** or **keep** — before touching it. Use `ask_user_input_v0` for a simple choice; a plain question if it needs more context. Don't infer "remove" from the original "clean this up" ask — that authorized structure work, not compat-code deletion.
4. Re-run `make fmt`, the full scoped gates, and any affected platform/feature
   checks after removal.

### Phase 6 — Final gate & summary

1. Full gate suite green (`make fmt`, `make check`, `make lint`, `make test`)
   is the actual completion criterion. Use `SCCACHE_DISABLE=1` only as a
   documented fallback.
2. Re-measure size for changed paths (`tokei <path> --sort lines`) to report a real before/after, not an impression.
3. Summarize:
    - **Splits made**: before path → after path(s), intra-crate or cross-crate, one-line reason.
    - **Size before → after** per unit (line counts from `tokei`/`wc -l`).
    - **Mod-cycle check**: confirmed clean, or what was restructured to avoid one.
    - **Dead code removed**: what, how confirmed dead.
    - **Legacy/back-compat**: what was found, what the user decided, what was actually removed/kept.
    - **Gate status**: final pass/fail per gate, per affected crate and feature
      scope.
    - **Deferred items**: design questions or cross-crate proposals not yet actioned.
    - **Suggested next step**: if concurrency/memory/perf wasn't in scope here, note that `rust-verify-harden` is the natural follow-up now that structure is settled.

## Notes

- Splitting preserves all behavior; only Phase 5 touches removal, and only for provably dead code or explicitly approved legacy cleanup.
- If a session must pause mid-reorg, only pause at a green checkpoint with a
  clearly described working-tree diff — never hand back a red gate or an
  unexplained half-split as "in progress."
- A "lean" result that ignores the crate's existing module conventions isn't actually lean for that codebase.
- Cross-crate moves and legacy-code deletion are the two things this skill never does silently — both need an explicit go-ahead.
- Git: do not commit or push unless explicitly instructed. Keep each split
  reviewable in the working tree.

## Example

User: "tidy up the Elph-Agent crate, if there is a fat module, just split it, make sure the gate is green"

Agent workflow:

1. Scope to `elph-agent`; baseline `make check -- -p elph-agent`,
   `make lint -- -p elph-agent`, and `make test -- -p elph-agent`, then record
   the `tokei` size baseline and git status.
2. Bloat scan (Phase 2) — flag oversized files/fns, duplication, dead code, legacy markers; exclude any generated files.
3. Plan splits leaf-first (Phase 3); call out any split that would cross into `elph-ai` as its own proposal.
4. Execute one split at a time, run the module-cycle check and scoped make
   gates after each, and leave the change reviewable without committing
   automatically (Phase 4).
5. Clean pass — dedupe/rename/tidy; ask before removing anything legacy/back-compat (Phase 5).
6. Final gate run + summary, with a note that `rust-verify-harden` is the natural next pass for hardening.
