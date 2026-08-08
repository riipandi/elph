---
name: rust-dep-audit
description: >-
    Audit a Rust workspace's dependencies for security advisories (RUSTSEC via cargo-audit),
    license compliance and ban/duplicate policy (cargo-deny), and staleness (cargo-outdated).
    Proposes a remediation plan classified by risk (patch/minor auto-safe, major flagged),
    applies safe bumps, and verifies cargo check/clippy/test stay green. Use when user asks
    to audit dependencies for vulnerabilities/CVEs, check license compliance, find outdated
    crates, or run a supply-chain/security pass on Cargo dependencies. Does not cover
    duplicate-version or unused-dependency hygiene — see cargo-workspace-hygiene for that.
metadata:
    scope: project
---

# Rust Dependency Audit

## Language

Split by destination:

- **In-chat responses, plans, summaries** — follow the language the user is currently using (Indonesian, English, etc). Crate names, versions, advisory IDs, license identifiers stay literal English.
- **Any documentation edits written to files** (`docs/**`, permanent comments, e.g. a `deny.toml` rationale comment) — **always English**, regardless of chat language.

## Purpose

Find and remediate supply-chain risk in the dependency tree: known vulnerabilities, disallowed/incompatible licenses, and dangerously stale crates — without breaking the build. Security/compliance/staleness concern only — not duplicate-version or unused-dep hygiene (that's `cargo-workspace-hygiene`), and not code-level refactoring.

## Scope

- Default scope: the whole workspace, since advisories and license terms apply to the resolved dependency graph as a whole, not per-crate.
- If the user names a specific crate, still resolve against the full workspace `Cargo.lock` — a vulnerable transitive dep pulled in by one member affects the shared build.

## Safety Rules

- **Patch/minor version bumps to fix an advisory are low-risk and can proceed**, but always re-verify gates immediately after — semver-compatible doesn't guarantee behavior-compatible.
- **Major version bumps are never silent.** Flag them with what changed (check the crate's CHANGELOG/release notes) and let the user decide — a major bump can be a bigger change than the audit alone justifies fixing right now.
- **No dependency removed or replaced without confirmation**, even if it has an unfixable advisory or disallowed license — removing/swapping a dep can ripple through calling code; propose the replacement, don't silently swap it in.
- **Gates green at every stopping point:** `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (or `make` equivalents) after each remediation, not just at the end.
- **No unprompted tool installs.** If `cargo-audit`/`cargo-deny`/`cargo-outdated` aren't installed, ask once before installing.
- **Don't loosen policy to make findings disappear.** If `deny.toml` already bans a license/crate, don't quietly widen the allow-list to silence a finding — surface the conflict to the user instead.
- **One remediation at a time**, gate-verified, same discipline as the other Rust skills in this set.

## Workflow

### Phase 1 — Baseline

1. Check tool availability: `cargo audit --version`, `cargo deny --version`, `cargo outdated --version`. Ask before installing any that are missing.
2. Confirm `Cargo.lock` is present and up to date (`cargo check --workspace` implicitly refreshes it; note if the lockfile was stale/missing).
3. Run baseline gates (`cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`) once before any changes — pre-existing failures aren't this skill's job to fix.
4. `git status -sb` — warn if dirty.
5. Check for an existing `deny.toml` — if absent and the user wants ongoing license/ban enforcement, note that one can be scaffolded (`cargo deny init`), but don't create it unprompted.

### Phase 2 — Security advisory scan

1. `cargo audit` against the workspace `Cargo.lock`. This reads RUSTSEC advisories for known vulnerabilities, unmaintained crates, and yanked versions.
2. For each finding, classify:
    - **Patched version available, semver-compatible** → safe to bump directly.
    - **Patched version available, major bump required** → flag with the advisory ID, severity, and what the major bump implies.
    - **No patch available / crate unmaintained** → flag for the user — options are usually wait, fork, or replace, all of which need a decision, not a default action.
3. Note advisories that are informational-only (e.g. "unmaintained" with no actual vulnerability) separately from actual CVEs/RUSTSEC vulnerabilities — don't conflate severity.

### Phase 3 — License compliance

1. If `deny.toml` exists: `cargo deny check licenses` (and `check bans`/`check sources` if those sections are configured too).
2. If no `deny.toml` exists: ask whether the user wants one scaffolded now, or just wants a one-off report — a one-off can use `cargo license` (if available) or `cargo tree --format "{p} {l}"`-style inspection as a fallback.
3. Flag any dependency whose license conflicts with the project's stated policy (if the user hasn't stated one, ask what's acceptable — e.g. permissive-only vs copyleft-tolerant — rather than assuming).
4. Don't silently add license exceptions to make a finding pass; surface the conflict.

### Phase 4 — Staleness scan

1. `cargo outdated --workspace --root-deps-only` for a first pass (deps the workspace itself pins), then `--depth <n>` or without `--root-deps-only` if the user wants the full transitive picture.
2. Classify each outdated dep: **patch/minor behind** (usually safe to bump), **major behind** (flag with changelog highlights if quickly checkable), **pinned intentionally** (matches a comment/reason in `Cargo.toml` — leave alone, don't "fix" a deliberate pin).

### Phase 5 — Remediation

1. Apply patch/minor bumps for advisory fixes and safe staleness findings, one dependency (or one small logical group) at a time.
2. Re-run the full gate suite after each bump. If a bump breaks a gate, revert just that bump (`git checkout -- Cargo.toml Cargo.lock` or a targeted `cargo update -p <crate> --precise <old-version>`) rather than chasing the break into unrelated code changes.
3. For major bumps / unmaintained crates / license conflicts: present the list with recommended next action per item and wait for the user's call — don't apply these unilaterally.

### Phase 6 — Verify

1. Full gate suite: `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
2. Re-run `cargo audit` and (if used) `cargo deny check` to confirm the addressed findings are actually gone and nothing new was introduced by the bumps.

### Phase 7 — Summary

- **Advisories found → fixed / flagged**: RUSTSEC ID, crate, severity, action taken or recommended.
- **License findings**: crate, license, conflict (if any), resolved or flagged.
- **Outdated deps bumped**: crate, old → new version, patch/minor/major.
- **Flagged for decision**: major bumps, unmaintained/no-patch crates, license conflicts — each with what a decision needs to consider.
- **Gate status**: pass/fail per gate.

## Notes

- This skill doesn't dedupe versions or remove unused deps — that's `cargo-workspace-hygiene`. If both are wanted, run hygiene first; a cleaner dep graph makes the audit's findings easier to interpret (fewer duplicate-version false alarms).
- Advisory/license findings in transitive deps you don't control directly still matter (they ship in the binary) — don't dismiss them just because they're not a direct dependency.
- Respect intentional pins — a version held back for a real reason (noted in a comment, or known from context) is not the same as staleness; ask before "fixing" it.
- Git: commit per remediation for rollback precision; push only if explicitly instructed.

## Example

User: "audit this workspace dependency, is there a CVE or not, is the license safe or not, some have been outdated for a long time"

Agent workflow:

1. Tool check, baseline gates, git status.
2. `cargo audit` → classify patch-fixable vs major vs no-patch.
3. `cargo deny check licenses` (or fallback report) → flag conflicts, don't auto-loosen policy.
4. `cargo outdated` → classify patch/minor/major/intentional-pin.
5. Apply safe patch/minor bumps one at a time, gate-verify each.
6. Final verify + summary, with major bumps and license conflicts listed for a decision.
