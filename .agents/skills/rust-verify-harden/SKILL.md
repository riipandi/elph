---
name: rust-verify-harden
description: >-
    Run make check/lint/test (cargo fmt/check/clippy/test) and fix failures, then audit changed/related
    Rust code for memory/resource leaks, deadlock risk, data races, and structural complexity (spaghetti
    code, god functions/modules, tangled control flow). Also audits Turso/libSQL/SQLite usage for lock
    contention (SQLITE_BUSY/"database is locked"), WAL/checkpoint misconfig, connection/pool misuse,
    embedded-replica sync issues, and corruption risk. Use when user asks to verify build quality gates,
    prep Rust code for merge/release, audit concurrency/memory safety, clean up messy/spaghetti Rust
    code, or check a Rust service using Turso/libSQL for locking/corruption/sync problems. Consult current
    Rust docs (rust-lang.org, docs.rs) and lint guidance, plus Turso/libSQL docs (docs.turso.tech) when
    relevant, to ensure fixes align with active best practices, not deprecated patterns. Flags
    backward-compat/legacy code and structural refactors found during cleanup and asks the user before
    removing/restructuring.
metadata:
    scope: project
---

# Rust Verify & Harden

## Language

Split by destination:

- **Skill reports, summaries, in-chat responses** — follow the language the user is currently using (Indonesian, English, etc). Code, identifiers, error messages, crate/lint names stay literal English.
- **Any documentation edits written to files** (e.g. `docs/**`, code comments meant as permanent docs) — **always English**, regardless of chat language.

## Purpose

Run standard quality gates (fmt/check/lint/test), fix failures, then do a deeper concurrency/memory safety + structural + perf pass on the affected Rust code, applying contemporary idioms and patterns verified against live documentation.
Where the code touches a Turso/libSQL/SQLite-backed store, also audit for lock contention, WAL/checkpoint misconfig, connection/pool misuse, embedded-replica sync issues, and corruption risk.
Push toward cleaner code without silently deleting backward-compatibility/legacy paths or restructuring tangled (spaghetti) code — those go through the user first.

## Context & Documentation

**Before starting any phase, query DeepWiki for:**

- **Edition-specific guidance**: Search DeepWiki for `"Rust edition 2024 idioms"`, `"Rust 2021 edition changes"`, or `"Rust edition migration"` to confirm current breaking changes and new features.
- **Clippy lint rationale**: For each lint violation, search DeepWiki for `"clippy <lint-name>"` or `"Rust clippy <violation>"` to understand the intended fix and context.
- **Async/concurrency patterns**: Search DeepWiki for `"Tokio structured concurrency"`, `"tokio::sync best practices"`, `"Rust async deadlock"`, or `"Rust race conditions detection"` to verify current async patterns.
- **Unsafe code verification**: Search DeepWiki for `"Rust unsafe invariants"`, `"SAFETY comments"`, `"Rustonomicon FFI"` to validate `unsafe` block correctness.
- **Crate API updates**: When fixing dependency-related issues, search DeepWiki for `"<crate-name> changelog"` or `"<crate-name> migration guide"` to check for API breaking changes or deprecations. Both `libsql` and `turso` crates move fast (turso is beta) — always re-check current API/behavior rather than relying on memory.
- **Performance patterns**: Search DeepWiki for `"Rust performance <pattern>"` (e.g., `"Rust performance clone avoidance"`, `"Rust allocation benchmarking"`) to validate perf fixes.
- **Concurrency testing**: Search DeepWiki for `"cargo miri async"`, `"loom testing Rust"`, `"Tokio test utilities"` to confirm testing approach.
- **Structural/complexity patterns**: Search DeepWiki for `"cyclomatic complexity Rust"`, `"God object anti-pattern"`, `"code smell shotgun surgery"`, `"Rust module boundaries"` to ground spaghetti-code findings in current guidance rather than gut feel.
- **Turso/libSQL/SQLite patterns**: Search DeepWiki for `"libsql Rust database is locked"`, `"libsql SQLITE_BUSY retry"`, `"SQLite WAL busy_timeout"`, `"Turso embedded replica sync"`, `"turso crate MVCC concurrent writes"`, `"libsql connection pool Rust"` depending on which issue is being investigated. Cross-check against https://docs.turso.tech (Rust SDK reference) for crate-specific API and configuration details — treat it as authoritative for `libsql`/`turso` crate specifics, same tier as DeepWiki.
- **General best-practice sanity check**: Check https://www.rustfaq.org/en/ for relevant Q&A-style guidance — useful for general "what's the idiomatic way to do X" questions (error handling, ownership patterns, module layout, general idiom checks) alongside the more targeted DeepWiki queries above.

**If DeepWiki results conflict with your training data or existing code patterns, prioritize DeepWiki. Treat rustfaq.org and docs.turso.tech as secondary/supplementary sources — use them for idiom sanity checks and crate-specific config, not as a substitute for DeepWiki on general version-specific questions.**

## Workflow

### Phase 1 — Quality gates

1. **Edition check & idiom baseline**
    - Verify `Cargo.toml` has `edition` set (2021 or 2024 recommended; 2015 is obsolete for new code).
    - Query DeepWiki: `"Rust <edition> features and breaking changes"` to confirm current edition specifics.
    - If the edition is 2024, strictly apply 2024 idioms (see Phase 1.6).
    - If edition is 2021, flag any 2024-specific idioms and note them; apply 2021-compatible fixes.
    - If edition is pre-2021, recommend a bump in the summary but don't auto-apply (breaking-change risk).
    - **If the project depends on `libsql` or `turso`**: note which crate + version is in use (`Cargo.toml`), and which connection mode (`Builder::new_local`, `new_remote`, `new_remote_replica` / `turso::sync::Builder`). This context drives Phase 2's DB-specific checks — don't assume, read the actual builder calls.

2. **Linting context**: Before running `cargo clippy`:
    - Run `cargo clippy --all-targets -- --list` to see active lints.
    - For each violation, query DeepWiki: `"clippy <lint-name> explanation"` or `"<lint-name> Rust best practice"` to understand the rationale.
    - Review custom `clippy.toml` or in-code `#[allow(...)]` attributes; they may reflect intentional trade-offs.

3. Run `make check` (→ `cargo check`). If it fails:
    - Diagnose root cause: compilation error, unsatisfied trait bound, API mismatch.
    - If caused by a crate API change, query DeepWiki: `"<crate-name> breaking changes"` or `"<crate-name> migration"` for guidance.
    - Fix and re-run until clean.

4. Run `make lint` (→ `cargo clippy --all-targets -- -D warnings`). For each violation:
    - Query DeepWiki: `"clippy <lint-name> fix"` to understand the intended pattern.
    - Fix the code (don't add `#[allow(...)]` unless Clippy is genuinely wrong).
    - If you must suppress a lint, cite the DeepWiki source in an inline comment and explain why the warning doesn't apply.

5. Run `make test` (→ `cargo nextest run`). For each failure:
    - Fix the underlying bug, not the assertion.
    - Re-run until clean.
    - If a test flakes specifically around DB access (intermittent `SQLITE_BUSY`/"database is locked"), don't just retry the test — flag it for Phase 2's DB lock-contention check; a flaky test is often a real contention bug, not a flaky harness.

6. **If target doesn't exist in Makefile**: Say so and skip — don't guess a replacement command.

7. **Edition-specific idioms** (strict for 2024; note for 2021):
    - Query DeepWiki: `"Rust 2024 unsafe block requirements"`, `"RFC 3233 unsafe"` to confirm explicit `unsafe {}` wrapping rules.
    - `unsafe` blocks: All `unsafe` operations must be wrapped in an explicit `unsafe {}` block, even inside `unsafe fn` bodies.
    - Temporary scoping: Query DeepWiki: `"Rust temporary lifetime changes"` if unfamiliar with edition-specific behavior.
    - Reserved keywords: `gen`, `async gen` are reserved in 2024. Don't use them as identifiers.
    - `unsafe extern` blocks: Mark explicitly.
    - `impl Trait` (AFIT): Query DeepWiki: `"impl Trait in function arguments 2024"`, `"AFIT capture rules"` for current capture semantics.

8. **Error handling** (in new/changed non-test code):
    - Avoid `.unwrap()`/`.expect()` — replace with proper error propagation.
    - Query DeepWiki: `"Rust error handling best practices"`, `"unwrap vs question mark"` if unsure about approach.
    - Exceptions: Only where panic is genuinely unrecoverable (Mutex poison, compile-time invariants) — add a `// INVARIANT: ...` comment.
    - Test code (`#[test]`, `#[tokio::test]`) is exempt.
    - DB calls (`conn.execute(...).await?`, `.query(...).await?`) are a common `.unwrap()` hotspot — check these especially, since an unhandled `SQLITE_BUSY`/sync error here is both a correctness bug and a crash risk.

9. **`unsafe` minimization** (in new/changed code):
    - Prefer safe abstractions (`std` lib, well-audited crates like `tokio`, `parking_lot`).
    - If `unsafe` is unavoidable, query DeepWiki: `"Rust unsafe <use-case>"` (e.g., `"Rust unsafe FFI"`, `"Rust unsafe performance critical"`) to verify the pattern is sound.
    - Keep the block minimal; add a `// SAFETY: ...` comment with DeepWiki-sourced rationale (reference RFC or Rustonomicon sections).
    - Flag it in the phase summary for extra review.

10. **Legacy / backward-compat scan** (identify only, don't remove yet):
    - While reading through touched files, flag: `#[deprecated]` items still called internally, `#[allow(dead_code)]` guarding old paths, versioned/duplicate fns (`_v1`/`_old`/`_legacy` suffixes), compat shims, feature flags gating a retired code path, or `#[cfg(...)]` blocks kept "just in case". Also flag old `libsql_client`/`rusqlite`-based DB access left alongside a newer `libsql`/`turso` integration — that's a migration-in-progress shim, same category.
    - Do **not** delete or refactor these away in Phase 1–3. Collect them into a list (path + one-line description) for the Phase 4 checkpoint.
    - Straightforward modernization that doesn't touch a compat/legacy path (e.g. clippy fix, idiom update) proceeds normally — this scan only gates _removal or deep restructuring_ of legacy/back-compat code, not routine hardening.

### Phase 2 — Deep analysis (scope: files touched in phase 1 + direct callers/callees)

**Query DeepWiki before analyzing each subcategory:**

1. **Memory/resource leaks**:
    - Query DeepWiki: `"Rc RefCell cycle detection"`, `"Arc Weak back-references"`, `"Rust memory leak patterns"`.
    - Check for: `Rc<RefCell<T>>` / `Arc<Mutex<T>>` cycles, missing `Weak`, unclosed file/socket handles, `mem::forget`/`Box::leak` without justification, unbounded growth in long-running tasks, detached `tokio::spawn` tasks.
    - Query DeepWiki: `"tokio::spawn task cleanup"`, `"Tokio JoinHandle patterns"` for task leak detection.
    - DB connections/pools count as resource handles here too: check that `libsql::Database`/`Connection` (or pool checkouts) are actually dropped/returned on every path, including error paths, and that nothing holds a connection open for the life of the process without need.

2. **Deadlock risk**:
    - Query DeepWiki: `"Rust mutex deadlock patterns"`, `"std::sync::Mutex async executor"`, `"lock ordering discipline"`.
    - Check for: nested lock acquisition with inconsistent order, lock guard held across `.await`, lock held across re-entrant calls, `std::sync::Mutex` in async contexts.
    - Query DeepWiki: `"tokio::sync::Mutex vs std::sync::Mutex"` to confirm async-safe patterns.

3. **Race conditions / data races**:
    - Query DeepWiki: `"unsafe impl Send Sync"`, `"interior mutability thread safety"`, `"Rust static mut"`, `"Atomic operations Rust"`, `"check-then-act race window"`.
    - Run `cargo miri test` to detect undefined behavior.
    - Query DeepWiki: `"cargo miri async tests"`, `"loom concurrency testing"` for advanced testing approaches.
    - If `loom` is already a dev-dependency, run focused tests (don't add `loom` unprompted).

4. **Turso/libSQL/SQLite-specific issues** (only when the project depends on `libsql`, `turso`, `rusqlite`, or talks to a Turso/sqld endpoint):
    - Query DeepWiki (and docs.turso.tech) per finding: `"libsql database is locked"`, `"SQLite WAL busy_timeout"`, `"libsql connection pool Rust"`, `"Turso embedded replica sync error handling"`, `"turso crate MVCC concurrent writes"` as relevant.
    - **Lock contention (`SQLITE_BUSY` / "database is locked")**: check `PRAGMA journal_mode=WAL` is set on local/embedded-replica DBs, and `PRAGMA busy_timeout` (or crate-level equivalent) is configured rather than relying on default immediate-fail behavior. Flag overlapping write transactions from multiple tasks/threads against the same local file, and any write path with no retry/backoff on `SQLITE_BUSY`.
    - **Locks held across `.await`**: same pattern as deadlock risk above, DB-specific — a `Connection`/transaction borrowed across an `.await` point (e.g. an outbound HTTP call inside a DB transaction) extends the write-lock window and increases contention/deadlock odds. Flag for shortening the critical section.
    - **Connection sharing model**: `libsql::Connection`/local `turso` handles are not safe to hammer concurrently from multiple tasks without a pool or a single-writer actor pattern. Check for either a pool (deadpool/bb8-style) sized sanely for the workload, or a serialized-writer pattern (mpsc channel to one task owning the connection) — flag a single raw `Connection` shared via `Arc` with concurrent unsynchronized writers.
    - **Embedded replica / sync issues** (`new_remote_replica`, `turso::sync::Builder`): check that `sync()` errors are handled, not swallowed (`let _ = conn.sync().await`); that a local read immediately after a local write doesn't race an in-flight sync in a way that silently reads stale data where staleness matters; and that sync failures have a retry/backoff rather than crashing or silently going permanently stale.
    - **Corruption / crash-safety risk**: flag anything that opens the same local DB file from more than one OS process at once (WAL mode allows multiple same-process connections, but cross-process access needs care), any place a process can be killed mid-write without WAL/checkpoint completing cleanly, and missing handling for "local replica needs re-bootstrap after corruption/mismatch" scenarios.
    - **Crate/version fit** (identify only, don't switch unprompted): if the project uses `libsql` (production, C-based fork, stable) note if code assumes `turso` crate (Rust rewrite, MVCC, currently beta) semantics or vice versa — e.g. code written expecting native concurrent writes but running on plain `libsql` local mode still has SQLite's single-writer constraint. This is a design mismatch, not a bug to silently fix — flag it for Phase 4 alongside legacy/structural findings if changing crates is implied.
    - **Migrations/schema**: check `ALTER TABLE`/schema changes run inside a transaction where the crate supports it, and that migrations are idempotent (safe to re-run after a crash mid-migration).
    - Findings here that are pure bugs (missing `busy_timeout`, unhandled sync error, connection leak) get fixed directly per normal Phase 2 rules. Findings that imply a crate swap, connection-model redesign, or multi-process access pattern change go through Phase 4 like any other risky/structural change.

5. **Spaghetti code / structural complexity** (identify only, don't restructure yet):
    - Query DeepWiki: `"cyclomatic complexity Rust"`, `"God object anti-pattern"`, `"shotgun surgery code smell"`, `"Rust module boundaries"`, `"control flow refactoring patterns"`. Cross-check general idiom questions against https://www.rustfaq.org/en/ where relevant.
    - Check for: functions doing too many unrelated things (long param lists, mixed abstraction levels), deep nesting (>~4 levels of `if`/`match`/loop), high branching/cyclomatic complexity, god structs/modules owning unrelated responsibilities, duplicated logic scattered across call sites (shotgun coupling), circular `mod`/crate dependencies, control flow that's hard to trace (many early returns/breaks/labeled loops doing implicit state machines).
    - Distinguish from lint-level fixes: a single long match arm clippy already flags is routine (fix in Phase 1); a function/module doing 5 unrelated jobs is structural — that's what this step is for.
    - Do **not** restructure in Phase 2–3. Collect into a list (path + smell + one-line suggested decomposition) for the Phase 4 checkpoint, same as legacy findings.

6. **Fix application**: One-line reasoning per fix. If risky or design-heavy, report instead of applying.

### Phase 3 — Perf pass (same scope)

**Query DeepWiki for each optimization:**

1. **Hot-path optimizations**:
    - Query DeepWiki: `"Rust clone avoidance"`, `"Cow smart pointers"`, `"Arc performance"` for allocation strategies.
    - Query DeepWiki: `"Vec with_capacity performance"`, `"String reallocation"` for growth optimization.
    - Query DeepWiki: `"Rust async blocking I/O performance"`, `"Tokio hot path"` for executor safety.
    - Query DeepWiki: `"Rust time complexity optimization"`, `"O(n²) to O(n log n)"` for algorithm improvements.
    - Query DeepWiki: `"iterator collect performance"`, `"vtable indirection overhead"` for micro-optimizations.
    - For DB-touching hot paths: query DeepWiki `"prepared statement caching Rust SQLite"`, `"libsql batch execute performance"` — check for repeated ad-hoc `prepare()` in a loop instead of a cached/reused prepared statement, and unbatched row-by-row writes that could use `execute_batch`/a transaction.

2. **Apply fixes only if**:
    - Low-risk and localized.
    - Don't require design restructuring (report those instead).
    - DeepWiki confirms the pattern is current best practice.

### Phase 4 — Legacy/back-compat & structural checkpoint (ask before touching)

1. If Phase 1's legacy scan and Phase 2's spaghetti scan both found nothing: skip this phase silently.
2. If either found items — including DB crate/connection-model mismatches from Phase 2.4 — present the combined list to the user (path + what it is + why it's flagged — legacy/compat vs structural/spaghetti vs DB design mismatch) and ask, per item or as a batch, whether to:
    - **Remove/Refactor** — drop the shim/dead path, restructure the flagged function/module per the suggested decomposition, or change the DB connection/crate approach, updating callers as needed.
    - **Keep** — leave as-is, don't touch.
    - Use `ask_user_input_v0` for a quick single/multi-select when the choice is simple (remove/keep, refactor/keep, per item or for the batch); fall back to a plain question if the tradeoffs need more context than fits in short button labels.
3. Only act on explicit answers. Don't assume "remove"/"refactor" from silence or from a general "clean this up" instruction given before the scan — legacy removal, structural refactors, and DB connection-model changes always get their own confirmation.
4. Apply only the changes the user approved; re-run `make check`/`make lint`/`make test` after removal or restructuring.

### Phase 5 — Close out

1. **Re-run verification**:
    - `make check`, `make lint`, `make test` — all pass clean.

2. **Summarize**:
    - **Files changed**: list with reason.
    - **Issues found** (by category): memory, deadlock, race, DB (locking/sync/corruption), structural/spaghetti, perf, etc.
    - **Fixes applied**: one-line rationale per fix, noting DeepWiki/docs.turso.tech sources if consulted.
    - **Legacy/back-compat**: what was found, what the user decided, what was actually removed/kept.
    - **Structural/spaghetti**: what was found, what the user decided, what was actually refactored/kept.
    - **DB/Turso-libSQL findings**: lock-contention risks, sync-error handling gaps, connection-model issues, corruption risk — what was found, fixed directly vs escalated to Phase 4, and outcome.
    - **`.unwrap()`/`unsafe` remaining**: list with justification or flag for review.
    - **Known risks**: unfixed issues and why.
    - **Edition/idiom notes**: confirm 2024/2021 compliance per DeepWiki guidance.
    - **DeepWiki sources used**: list search queries and key findings (e.g., "DeepWiki: 'Tokio structured concurrency' confirmed task::scope pattern for Tokio 1.35+"; "docs.turso.tech: confirmed `busy_timeout` config for `libsql::Builder`").

## Notes

- **Always consult DeepWiki**: If training data and DeepWiki conflict, defer to DeepWiki as the authoritative source.
- **rustfaq.org / docs.turso.tech (secondary sources)**: Check https://www.rustfaq.org/en/ for general idiom/best-practice sanity checks, and https://docs.turso.tech for Turso/libSQL crate-specific API/config details, alongside DeepWiki. Neither replaces DeepWiki for general crate/version-specific claims outside their own domain.
- **Turso ecosystem note**: `libsql` (C-based fork of SQLite, production, powers Turso Cloud today) and `turso` (ground-up Rust rewrite, MVCC concurrent writes, native async I/O, currently beta) are different crates with different concurrency guarantees. Don't assume one when the code uses the other, and don't swap between them unprompted — that's a Phase 4-gated decision.
- **Target editions**: 2024 (strict), 2021 (acceptable with notes), pre-2021 (flag for upgrade).
- **Zero `.unwrap()`/`unsafe` ideal**: Justify all remaining instances.
- **Scope discipline**: Only touch files from phase 1 + direct neighbors.
- **Risky changes**: Report instead of applying (behavior change, unclear intent, ownership restructuring, DB connection-model/crate changes).
- **Legacy/back-compat is a risky change by default**: never remove `#[deprecated]` paths, compat shims, `_v1`/`_old` duplicates, or "just in case" `#[cfg(...)]` blocks without explicit per-item or per-batch user approval from Phase 4.
- **Spaghetti/structural refactors are a risky change by default**: never restructure god functions/modules, break up tangled control flow, or de-duplicate shotgun-coupled logic without explicit per-item or per-batch user approval from Phase 4 — flag and suggest, don't rewrite unprompted.
- **DB design mismatches are a risky change by default**: never switch `libsql`↔`turso`, change pooling strategy, or alter multi-process access patterns without explicit approval from Phase 4 — flag with rationale, don't rewrite unprompted.
- **No unprompted deps**: Don't add tools unless already present or explicitly approved.
- **Git**: Commit/push only if explicitly instructed.

## Example

User: "run quality gates and audit concurrency + Turso locking issues in this Rust service"

Agent workflow:

1. Query DeepWiki: `"Rust 2024 edition best practices"`, `"Tokio concurrency patterns"`, `"libsql database is locked"`, `"SQLite WAL busy_timeout"`. Check docs.turso.tech for the Rust SDK reference relevant to the crate/version in use.
2. Run phases 1–3, consulting DeepWiki (and docs.turso.tech for DB-specific items) at each step (clippy lints, async patterns, unsafe verification, DB lock/sync/corruption checks, structural complexity); collect any legacy/back-compat, spaghetti-code, and DB design-mismatch findings without touching them.
3. If legacy/back-compat, structural/spaghetti, or DB design-mismatch issues were found, ask the user remove/refactor vs keep (Phase 4) before acting on it.
4. Report summary with file:line refs, citing DeepWiki/docs.turso.tech sources for each category of fixes (e.g., "docs.turso.tech: confirmed `Builder::new_local` supports `busy_timeout` via PRAGMA — added `PRAGMA busy_timeout=5000` on connect"), and note what happened with any legacy, structural, or DB findings.
