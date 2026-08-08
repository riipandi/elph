---
name: cargo-workspace-hygiene
description: >-
    Audit a Rust workspace for dependency hygiene: duplicate crate versions across
    members, unused/dead dependencies, feature-flag bloat, and un-unified workspace
    dependency tables. Proposes and safely applies fixes (unify to [workspace.dependencies],
    drop unused deps, trim feature sets), verifying cargo check/clippy/test stay green.
    Use when user asks to clean up Cargo.toml(s), dedupe dependency versions, find unused
    crates, unify workspace deps, or audit workspace dependency structure. Does not cover
    security advisories or license compliance — see rust-dep-audit for that.
metadata:
    scope: project
---

# Cargo Workspace Hygiene

## Language

Split by destination:

- **In-chat responses, plans, summaries** — follow the language the user is currently using (Indonesian, English, etc). Crate names, paths, versions, flags stay literal English.
- **Any documentation edits written to files** (`docs/**`, permanent comments) — **always English**, regardless of chat language.

## Purpose

Keep the workspace's dependency graph and `Cargo.toml` files lean: no duplicate versions of the same crate pulled in by different members for no reason, no unused dependencies, no feature-flag creep, and a single source of truth for shared deps via `[workspace.dependencies]`. Structural/hygiene concern only — not vulnerability or license auditing (that's `rust-dep-audit`), and not code-level splitting (that's `rust-lean-refactor`).

## Scope

- Default scope: the whole workspace's `Cargo.toml` files (root + all member crates), since duplication and unification are inherently cross-crate concerns.
- If the user names a specific crate, still check its deps against the rest of the workspace for duplicates — hygiene issues are relational, not per-crate.

## Safety Rules

- **No version bumps without a call-out.** Deduping to a shared version is only safe when semver-compatible for all consumers; if unifying would force a minor/major bump on some crate, propose it explicitly and let the user decide rather than silently bumping.
- **Unused-dependency removal needs confirmation if it's not obviously safe.** A dep with zero references found by tooling is a strong signal, but re-exports, macro-only usage, or feature-gated code paths can hide real usage — verify before removing, and if genuinely uncertain, ask rather than delete.
- **Gates green at every stopping point:** `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (or the `make` equivalents) must pass after each change, not just at the end.
- **No unprompted tool installs.** If `cargo-machete`/`cargo-udeps`/`cargo-hakari` aren't already installed, ask once before installing (they're dev-only, low-risk, but still an environment change) rather than silently running `cargo install`.
- **One category of change at a time** — don't mix a version-unification pass with a feature-trim pass with an unused-dep removal pass in one unverified diff. Verify gates between categories.
- **Legacy/back-compat deps** (a dep kept only for an old code path) follow the same ask-before-remove rule as other skills — don't fold "unused dependency" and "deliberately-kept-for-legacy" into the same silent bucket.

## Workflow

### Phase 1 — Baseline

1. Confirm workspace layout: `cargo metadata --no-deps --format-version 1 | jq '.workspace_members'` or just read the root `Cargo.toml`'s `[workspace] members`.
2. Run baseline gates: `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Note pre-existing failures — not this skill's job to fix unrelated red gates.
3. `git status -sb` — warn if dirty before starting.
4. Check tool availability: `cargo machete --version`, `cargo udeps --version`, `cargo hakari --version`. If missing and needed for a phase below, ask before installing.

### Phase 2 — Duplicate version scan

1. `cargo tree --duplicates` (or `cargo tree -d` short form) at the workspace root — lists every crate pulled in at more than one version.
2. For each duplicate, `cargo tree -i <crate>` to see which members pull which version and why (direct vs transitive).
3. Classify:
    - **Direct + compatible** — two members pin different semver-compatible versions directly → safe to unify via `[workspace.dependencies]`.
    - **Direct + incompatible** — different major versions pinned on purpose or due to a real breaking change upstream → flag for the user, don't force.
    - **Transitive-only** — duplication comes from third-party deps, not the workspace's own choices → usually not actionable; note it, don't chase it.
4. Produce a list: crate → versions found → members affected → proposed action (unify / flag / ignore-transitive).

### Phase 3 — Unused dependency scan

1. `cargo machete` (fast, no build required) as the first pass across the workspace.
2. Cross-check anything it flags with `rg "\buse\s+<crate>|<crate>::"` across the member's `src/` — macro-only or re-export-only usage can produce false positives.
3. If `cargo-udeps` is available and the user wants a deeper pass (it needs nightly + a full build), run `cargo +nightly udeps --workspace` for confirmation on ambiguous cases.
4. Classify each finding: **confirmed unused** (remove), **ambiguous** (ask), **used only in a legacy/back-compat path** (flag, don't remove without approval).

### Phase 4 — Feature-flag audit

1. For each member, check `Cargo.toml` `[features]` and `default = [...]` for flags that are no longer referenced by any `#[cfg(feature = "...")]` in the crate (`rg 'cfg\(feature'`).
2. Check for feature unification problems across the workspace — a feature enabled by one member's dev-deps leaking into another member's default build (`cargo tree -e features` or `cargo hakari` if the workspace already uses it).
3. Flag: dead feature flags (safe-ish to remove after confirming no downstream consumer outside the workspace depends on them), and unification issues (propose `cargo-hakari` adoption only if the user asks — don't introduce a new hakari crate unprompted).

### Phase 5 — Unify workspace dependency table

1. For deps used by 2+ members at the same (or now-unified) version, propose moving them into root `[workspace.dependencies]` and switching members to `dep = { workspace = true }`.
2. Apply one dependency (or one small logical group) at a time; re-run `cargo check --workspace` after each.
3. Preserve any per-member feature differences explicitly (`dep = { workspace = true, features = [...] }`) rather than collapsing them if they're genuinely different.

### Phase 6 — Verify

1. Full workspace gate suite: `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (or `make check/lint/test` if that's the real wrapper).
2. Re-run `cargo tree --duplicates` to confirm the targeted duplicates are actually gone (and no new ones were introduced by the unification).

### Phase 7 — Summary

- **Duplicates found → resolved / flagged**: crate, versions, action taken.
- **Unused deps removed**: crate, member, how confirmed unused.
- **Feature flags trimmed**: flag, member, why dead.
- **Workspace-dependencies unified**: list of deps moved to `[workspace.dependencies]`.
- **Flagged, not touched**: incompatible-version duplicates, ambiguous unused deps, legacy-path deps — with the reason and what a decision would need.
- **Gate status**: pass/fail per gate, workspace-wide.

## Notes

- This skill doesn't run `cargo audit` or license checks — that's `rust-dep-audit`. If the user wants both, run this one first (structural cleanup can change which versions exist, which changes what the audit sees).
- Transitive-only duplicates from third-party crates are usually not worth chasing — don't manufacture busywork trying to eliminate them.
- Respect existing workspace conventions (e.g. if some members intentionally pin an older version for compatibility with an external consumer) — ask rather than "fixing" what might be deliberate.
- Git: commit per logical change for easy rollback; push only if explicitly instructed.

## Example

User: "check this workspace's dependencies, are there any duplicate versions, are there any unused ones, just unify them into workspace.dependencies"

Agent workflow:

1. Baseline gates + `cargo tree -d` + tool availability check.
2. Phase 2: classify duplicates (unify-safe vs flag vs transitive-noise).
3. Phase 3: `cargo machete` + manual cross-check for false positives.
4. Phase 4: dead feature flags.
5. Phase 5: move shared deps to `[workspace.dependencies]` one group at a time, gate-check between groups.
6. Phase 6–7: final verify + summary with what got flagged for a decision.
