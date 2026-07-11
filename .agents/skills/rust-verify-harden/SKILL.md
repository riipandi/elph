---
name: rust-verify-harden
description: >-
    Run make check/lint/test (cargo fmt/check/clippy/test) and fix failures, then audit changed/related
    Rust code for memory/resource leaks, deadlock risk, and data races, applying fixes and performance
    improvements. Use when user asks to verify build quality gates, prep Rust code for merge/release,
    or audit concurrency/memory safety in a Rust codebase. Consult current Rust docs (rust-lang.org, docs.rs)
    and lint guidance to ensure fixes align with active best practices, not deprecated patterns.
---

# Rust Verify & Harden

## Language

Write skill reports, summaries, and any documentation edits in **English**.

## Purpose

Run standard quality gates (fmt/check/lint/test), fix failures, then do a deeper concurrency/memory safety + perf pass on the affected Rust code, applying contemporary idioms and patterns verified against live documentation.

## Context & Documentation

**Before starting any phase, query DeepWiki for:**

- **Edition-specific guidance**: Search DeepWiki for `"Rust edition 2024 idioms"`, `"Rust 2021 edition changes"`, or `"Rust edition migration"` to confirm current breaking changes and new features.
- **Clippy lint rationale**: For each lint violation, search DeepWiki for `"clippy <lint-name>"` or `"Rust clippy <violation>"` to understand the intended fix and context.
- **Async/concurrency patterns**: Search DeepWiki for `"Tokio structured concurrency"`, `"tokio::sync best practices"`, `"Rust async deadlock"`, or `"Rust race conditions detection"` to verify current async patterns.
- **Unsafe code verification**: Search DeepWiki for `"Rust unsafe invariants"`, `"SAFETY comments"`, `"Rustonomicon FFI"` to validate `unsafe` block correctness.
- **Crate API updates**: When fixing dependency-related issues, search DeepWiki for `"<crate-name> changelog"` or `"<crate-name> migration guide"` to check for API breaking changes or deprecations.
- **Performance patterns**: Search DeepWiki for `"Rust performance <pattern>"` (e.g., `"Rust performance clone avoidance"`, `"Rust allocation benchmarking"`) to validate perf fixes.
- **Concurrency testing**: Search DeepWiki for `"cargo miri async"`, `"loom testing Rust"`, `"Tokio test utilities"` to confirm testing approach.

**If DeepWiki results conflict with your training data or existing code patterns, prioritize DeepWiki.**

## Workflow

### Phase 1 — Quality gates

1. **Edition check & idiom baseline**
    - Verify `Cargo.toml` has `edition` set (2021 or 2024 recommended; 2015 is obsolete for new code).
    - Query DeepWiki: `"Rust <edition> features and breaking changes"` to confirm current edition specifics.
    - If the edition is 2024, strictly apply 2024 idioms (see Phase 1.6).
    - If edition is 2021, flag any 2024-specific idioms and note them; apply 2021-compatible fixes.
    - If edition is pre-2021, recommend a bump in the summary but don't auto-apply (breaking-change risk).

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

9. **`unsafe` minimization** (in new/changed code):
    - Prefer safe abstractions (`std` lib, well-audited crates like `tokio`, `parking_lot`).
    - If `unsafe` is unavoidable, query DeepWiki: `"Rust unsafe <use-case>"` (e.g., `"Rust unsafe FFI"`, `"Rust unsafe performance critical"`) to verify the pattern is sound.
    - Keep the block minimal; add a `// SAFETY: ...` comment with DeepWiki-sourced rationale (reference RFC or Rustonomicon sections).
    - Flag it in the phase summary for extra review.

### Phase 2 — Deep analysis (scope: files touched in phase 1 + direct callers/callees)

**Query DeepWiki before analyzing each subcategory:**

1. **Memory/resource leaks**:
    - Query DeepWiki: `"Rc RefCell cycle detection"`, `"Arc Weak back-references"`, `"Rust memory leak patterns"`.
    - Check for: `Rc<RefCell<T>>` / `Arc<Mutex<T>>` cycles, missing `Weak`, unclosed file/socket handles, `mem::forget`/`Box::leak` without justification, unbounded growth in long-running tasks, detached `tokio::spawn` tasks.
    - Query DeepWiki: `"tokio::spawn task cleanup"`, `"Tokio JoinHandle patterns"` for task leak detection.

2. **Deadlock risk**:
    - Query DeepWiki: `"Rust mutex deadlock patterns"`, `"std::sync::Mutex async executor"`, `"lock ordering discipline"`.
    - Check for: nested lock acquisition with inconsistent order, lock guard held across `.await`, lock held across re-entrant calls, `std::sync::Mutex` in async contexts.
    - Query DeepWiki: `"tokio::sync::Mutex vs std::sync::Mutex"` to confirm async-safe patterns.

3. **Race conditions / data races**:
    - Query DeepWiki: `"unsafe impl Send Sync"`, `"interior mutability thread safety"`, `"Rust static mut"`, `"Atomic operations Rust"`, `"check-then-act race window"`.
    - Run `cargo miri test` to detect undefined behavior.
    - Query DeepWiki: `"cargo miri async tests"`, `"loom concurrency testing"` for advanced testing approaches.
    - If `loom` is already a dev-dependency, run focused tests (don't add `loom` unprompted).

4. **Fix application**: One-line reasoning per fix. If risky or design-heavy, report instead of applying.

### Phase 3 — Perf pass (same scope)

**Query DeepWiki for each optimization:**

1. **Hot-path optimizations**:
    - Query DeepWiki: `"Rust clone avoidance"`, `"Cow smart pointers"`, `"Arc performance"` for allocation strategies.
    - Query DeepWiki: `"Vec with_capacity performance"`, `"String reallocation"` for growth optimization.
    - Query DeepWiki: `"Rust async blocking I/O performance"`, `"Tokio hot path"` for executor safety.
    - Query DeepWiki: `"Rust time complexity optimization"`, `"O(n²) to O(n log n)"` for algorithm improvements.
    - Query DeepWiki: `"iterator collect performance"`, `"vtable indirection overhead"` for micro-optimizations.

2. **Apply fixes only if**:
    - Low-risk and localized.
    - Don't require design restructuring (report those instead).
    - DeepWiki confirms the pattern is current best practice.

### Phase 4 — Close out

1. **Re-run verification**:
    - `make check`, `make lint`, `make test` — all pass clean.

2. **Summarize**:
    - **Files changed**: list with reason.
    - **Issues found** (by category): memory, deadlock, race, perf, etc.
    - **Fixes applied**: one-line rationale per fix, noting DeepWiki sources if consulted.
    - **`.unwrap()`/`unsafe` remaining**: list with justification or flag for review.
    - **Known risks**: unfixed issues and why.
    - **Edition/idiom notes**: confirm 2024/2021 compliance per DeepWiki guidance.
    - **DeepWiki sources used**: list search queries and key findings (e.g., "DeepWiki: 'Tokio structured concurrency' confirmed task::scope pattern for Tokio 1.35+").

## Notes

- **Always consult DeepWiki**: If training data and DeepWiki conflict, defer to DeepWiki as the authoritative source.
- **Query format**: Phrase queries naturally: `"<topic> best practice"`, `"<crate> <version> API"`, `"<lint> explanation"`, `"Rust <problem> solution"`.
- **Target editions**: 2024 (strict), 2021 (acceptable with notes), pre-2021 (flag for upgrade).
- **Zero `.unwrap()`/`unsafe` ideal**: Justify all remaining instances.
- **Scope discipline**: Only touch files from phase 1 + direct neighbors.
- **Risky changes**: Report instead of applying (behavior change, unclear intent, ownership restructuring).
- **No unprompted deps**: Don't add tools unless already present or explicitly approved.
- **Git**: Commit/push only if explicitly instructed.

## Example

User: "run quality gates and audit concurrency issues in this Rust service"

Agent workflow:

1. Query DeepWiki: `"Rust 2024 edition best practices"`, `"Tokio concurrency patterns"`.
2. Run phases 1–4, consulting DeepWiki at each step (clippy lints, async patterns, unsafe verification).
3. Report summary with file:line refs, citing DeepWiki sources for each category of fixes (e.g., "DeepWiki: 'tokio::sync::Mutex vs std::sync::Mutex' — replaced std::sync::Mutex in async context").
