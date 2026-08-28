---
name: rust-dep-audit
description: >-
    Audit the Elph Rust workspace's dependencies for security advisories (RUSTSEC via
    cargo-audit), license compliance and ban/source policy (cargo-deny), staleness
    (cargo-outdated), yanked packages, and risky git/path/patch sources. Proposes a
    remediation plan classified by risk, applies changes only when requested, and verifies
    the repository's make check/lint/test gates. Use when user asks
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

Find and, when explicitly requested, remediate supply-chain risk in the resolved
dependency graph: known vulnerabilities, unmaintained or yanked packages,
disallowed licenses/sources, suspicious overrides, and dangerously stale crates.
Separate facts from policy decisions and never make a dependency change merely to
silence a report. This is security/compliance/staleness work only — not
duplicate-version or unused-dependency hygiene (`cargo-workspace-hygiene`) and
not code-level refactoring.

### Elph workspace baseline

Verify the current workspace before auditing. Elph uses resolver 2, edition 2024,
Rust 1.97 at workspace level, and publishable crates with their own MSRV
(`elph-ai` currently 1.88 and `elph-agent` currently 1.89). The normal graph
includes `elph`, `elph-agent`, `elph-ai`, `elph-tui`, `floppy`, and `rendown`.
The root has a large `[workspace.dependencies]` table and a local
`[patch.crates-io]` override for `iocraft` in `vendor/iocraft`.

The workspace also uses the pinned `turso` crate (`=0.8.0-pre.7`) for local
storage. Record this and all other exact pins, path dependencies, git sources,
patches, target-specific dependencies, optional features, and publish metadata
before interpreting a finding.

## Scope

- Default scope: the whole workspace, since advisories and license terms apply to the resolved dependency graph as a whole, not per-crate.
- If the user names a specific crate, still resolve against the full workspace `Cargo.lock` — a vulnerable transitive dep pulled in by one member affects the shared build.

## Safety Rules

- **Audit and remediation are separate modes.** An audit request produces a
  report and plan only. Apply even a patch/minor bump only when the user asks to
  remediate or approves the proposed item.
- **Patch/minor version bumps to fix an advisory are usually lower-risk**, but
  semver compatibility does not guarantee behavior compatibility. Re-check the
  advisory's affected range and the crate's release notes.
- **Major version bumps are never silent.** Flag them with what changed (check the crate's CHANGELOG/release notes) and let the user decide — a major bump can be a bigger change than the audit alone justifies fixing right now.
- **No dependency removed or replaced without confirmation**, even if it has an unfixable advisory or disallowed license — removing/swapping a dep can ripple through calling code; propose the replacement, don't silently swap it in.
- **Use repository wrappers for gates:** `make fmt`, `make check`, `make lint`,
  and `make test` after each remediation. Forward package or feature selectors
  after `--` when a focused gate is useful, then run the full workspace gates
  before completion. Use `SCCACHE_DISABLE=1` only when sccache is unavailable
  or unhealthy and record that fact.
- **No unprompted tool installs.** If `cargo-audit`/`cargo-deny`/`cargo-outdated` aren't installed, ask once before installing.
- **Don't loosen policy to make findings disappear.** If `deny.toml` already bans a license/crate, don't quietly widen the allow-list to silence a finding — surface the conflict to the user instead.
- **One remediation at a time**, gate-verified, same discipline as the other
  Rust skills in this set.
- **Do not destroy parallel work.** Never use `git reset --hard` or a broad
  `git checkout --` to undo a failed update. Preserve unrelated edits and use
  targeted patches or ask when ownership is unclear.
- **Git is opt-in:** do not commit or push unless the user explicitly requests it.

## Workflow

### Phase 1 — Baseline

1. `git status --short` and `git diff --stat`; stop if ownership of existing
   edits is unclear.
2. Confirm workspace membership and package metadata with
   `cargo metadata --no-deps --format-version 1`. Record edition, MSRV,
   publish flags, package licenses, target dependencies, path/git sources,
   patches, and enabled default features.
3. Confirm `Cargo.lock` is present, tracked, and consistent. Run `make check`
   and inspect whether it changes the lockfile; do not hide lockfile drift.
4. Run baseline gates once: `make fmt`, `make check`, `make lint`, `make test`.
   Pre-existing failures are reported separately.
5. Check tools without installing them:
   `command -v cargo-audit cargo-deny cargo-outdated`.
   If a tool is unavailable, report the missing evidence and use a narrower
   fallback only where it is meaningful.
6. Check for `deny.toml`, `.cargo/config.toml`, CI audit jobs, and advisory
   ignore lists. Never assume an absent policy file means every license is
   acceptable.

### Phase 2 — Security advisory scan

1. Run `cargo audit` against the tracked workspace `Cargo.lock` when installed.
   Prefer machine-readable output (`--json`) for a precise report when supported,
   but retain the human-readable summary. Do not refresh or rewrite the
   advisory database as part of a code change without noting it.
2. Inspect `cargo audit` configuration and ignored advisories before accepting
   the result. An ignored advisory is still reported as policy debt, not treated
   as absent.
3. For each finding, classify:
    - **Patched version available, semver-compatible** → safe to bump directly.
    - **Patched version available, major bump required** → flag with the advisory ID, severity, and what the major bump implies.
    - **No patch available / crate unmaintained** → flag for the user — options are usually wait, fork, or replace, all of which need a decision, not a default action.
4. Note advisories that are informational-only (e.g. "unmaintained" with no
   actual vulnerability) separately from actual CVEs/RUSTSEC vulnerabilities.
   Include affected package, dependency path, fixed version, severity,
   exploitability in Elph's use, and whether the package is direct or transitive.

### Phase 3 — License compliance

1. If `deny.toml` exists, run the configured checks:
   `cargo deny check licenses`, `cargo deny check bans`, and
   `cargo deny check sources` as applicable. Respect `deny`, `warn`, `skip`,
   `allow`, source, and license exceptions already documented there.
2. If no `deny.toml` exists, inspect package metadata and use
   `cargo tree --format "{p} {l}"` or an installed license tool for a one-off
   inventory. Report that no enforceable project license policy exists; do not
   create `deny.toml` or invent an allow-list unprompted.
3. Check workspace packages as well as transitive packages, including the
   vendored `iocraft` patch and path dependencies. Distinguish SPDX expression
   parsing issues from actual policy conflicts.
4. Flag any conflict with the stated policy. If no policy is stated, present
   the choices (for example permissive-only versus copyleft-tolerant) instead of
   assuming one.
5. Do not silently add license exceptions or widen source allow-lists.

### Phase 4 — Staleness scan

1. Run `cargo outdated --workspace --root-deps-only` when available for direct
   workspace dependencies, then expand depth only when requested. Capture the
   report date and registry/tool version.
2. Check yanked versions and stale git revisions separately. A package being
   outdated is not itself a vulnerability.
3. Classify each dependency as **patch/minor behind**, **major behind**,
   **intentionally pinned**, **blocked by MSRV**, **blocked by platform**, or
   **not actionable because it is transitive-only**. Check comments, exact
   pins, local patches, and release notes before recommending an update.

### Phase 5 — Remediation

1. Apply changes only in remediation mode or after explicit approval. Prefer a
   targeted `cargo update -p <crate> --precise <version>` and review the full
   `Cargo.toml`/`Cargo.lock` diff. Do not run an unconstrained workspace update
   to fix one package.
2. For each update, check MSRV, feature changes, target support, local patches,
   public API changes, and release notes. Run `make fmt`, `make check`,
   `make lint`, and the relevant tests before the next update.
3. If an update fails, preserve the failed diff for diagnosis or use a targeted
   patch to restore only the agent-owned change. Never overwrite unrelated
   developer edits with a broad restore command.
4. For major bumps, unmaintained crates, no-patch advisories, yanked packages,
   source changes, or license conflicts: present options and wait for the
   user's decision.

### Phase 6 — Verify

1. Run `make fmt`, `make check`, `make lint`, and `make test`.
2. Re-run `cargo audit` and configured `cargo deny` checks. Re-run
   `cargo outdated` when a staleness update was made.
3. Inspect `git diff -- Cargo.toml Cargo.lock deny.toml .cargo` and confirm no
   advisory ignore, license exception, source allow-list, or unrelated
   dependency was introduced.

### Phase 7 — Summary

- **Advisories found → fixed / flagged**: RUSTSEC ID, crate, severity, action taken or recommended.
- **License findings**: crate, license, conflict (if any), resolved or flagged.
- **Outdated deps bumped**: crate, old → new version, patch/minor/major.
- **Flagged for decision**: major bumps, unmaintained/no-patch crates, license conflicts — each with what a decision needs to consider.
- **Gate status**: pass/fail per gate, package/feature scope, and whether
  `SCCACHE_DISABLE=1` or a platform-specific selector was needed.
- **Evidence gaps**: unavailable audit tools, offline registries, missing
  `deny.toml`, ignored advisories, and checks not run.

## Notes

- This skill doesn't dedupe versions or remove unused deps — that's `cargo-workspace-hygiene`. If both are wanted, run hygiene first; a cleaner dep graph makes the audit's findings easier to interpret (fewer duplicate-version false alarms).
- Advisory/license findings in transitive deps you don't control directly still matter (they ship in the binary) — don't dismiss them just because they're not a direct dependency.
- Respect intentional pins — a version held back for a real reason (noted in a comment, or known from context) is not the same as staleness; ask before "fixing" it.
- Git: do not commit or push unless explicitly instructed. Keep remediation
  changes reviewable and separable.

## Example

User: "audit this workspace dependency, is there a CVE or not, is the license safe or not, some have been outdated for a long time"

Agent workflow:

1. Tool check, baseline gates, git status.
2. `cargo audit` → classify patch-fixable vs major vs no-patch.
3. `cargo deny check licenses` (or fallback report) → flag conflicts, don't auto-loosen policy.
4. `cargo outdated` → classify patch/minor/major/intentional-pin.
5. Apply safe patch/minor bumps one at a time, gate-verify each.
6. Final verify + summary, with major bumps and license conflicts listed for a decision.
