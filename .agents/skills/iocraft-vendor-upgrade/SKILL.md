---
name: iocraft-vendor-upgrade
description: >-
    Safely upgrade the vendored `iocraft` crate at vendor/iocraft in the elph
    workspace to a newer upstream release. Use this whenever the user asks
    to bump/update/upgrade vendored iocraft, mentions a new iocraft release
    tag (iocraft-vX.Y.Z), or asks to sync vendor/iocraft with upstream
    ccbrown/iocraft. This is NOT a plain `cargo update` — iocraft is vendored
    with local customizations, so upstream changes must be diffed against the
    fork point and merged without silently discarding elph-specific patches.
    Always use this skill instead of blindly overwriting vendor/iocraft or
    copy-pasting upstream source.
---

# iocraft Vendor Upgrade (elph)

## Why this exists

`vendor/iocraft` in elph is a **vendored fork**, not a plain dependency pin. It exists because elph needed changes/customizations upstream doesn't have. A naive upgrade (delete + copy new upstream source) silently destroys those customizations. A naive "just patch the diff" also risks missing upstream fixes that touch the same files.

This skill treats the upgrade as a **three-way merge problem**: baseline (old upstream) -> ours (elph's customized vendor) -> theirs (new upstream). Never skip straight to step 6.

Gate-green rule: do not move to the next step until the current one is verified. Ask-before-remove rule: if applying upstream changes would drop or invalidate an elph customization, stop and ask the user — do not silently discard it. No-commit rule: this skill never runs `git commit` or `git push` — commit and push are done by the user only, always.

## Step 0 — Preconditions

- Confirm target release tag from the user (e.g. `iocraft-v0.8.5`) and repo: `ccbrown/iocraft`.
- Make sure the working tree is clean (`git status` in the elph repo) before touching `vendor/iocraft`. If dirty, stop and ask.
- Create a throwaway branch: `git checkout -b chore/vendor-iocraft-<version>`.

## Step 1 — Establish the baseline (fork point)

You need to know exactly which upstream commit/tag `vendor/iocraft` currently corresponds to.

1. Check `vendor/iocraft/Cargo.toml` for the version string, and check for any existing marker file (`vendor/iocraft/VENDOR.md`, `PATCHES.md`, `.vendor-version`, or a comment block at the top of modified files referencing an upstream commit/tag).
2. If no marker exists, treat this as a documentation gap: after finishing the upgrade, create `vendor/iocraft/VENDOR.md` recording upstream repo, baseline tag/commit, and the list of local patches — so the next upgrade doesn't have to re-derive this from scratch.
3. Resolve the baseline tag against upstream: `git ls-remote --tags https://github.com/ccbrown/iocraft.git` (or `gh release view iocraft-v<old> -R ccbrown/iocraft`) to get the exact commit SHA for the currently-vendored version.

## Step 2 — Inventory elph's customizations

Fetch pristine upstream source at the baseline commit and diff it against the current `vendor/iocraft` tree — this diff IS the customization set.

```bash
# in a scratch dir outside the elph repo
git clone --depth 1 --branch iocraft-v<OLD_VERSION> https://github.com/ccbrown/iocraft.git upstream-baseline

# diff pristine baseline vs elph's vendored copy (adjust source subpath if iocraft's repo has a workspace layout)
diff -ruN upstream-baseline/iocraft <elph-repo>/vendor/iocraft > /tmp/elph-iocraft-customizations.diff
```

Review this diff file by hand. For each hunk, note: which file, what it does, and *why* (infer from context/comments/commit history — `git log --follow -- vendor/iocraft/<file>` in the elph repo helps). This becomes your checklist for step 5.

If `vendor/iocraft/VENDOR.md` or `PATCHES.md` already documents the customizations, cross-check the diff against that doc instead of re-deriving blind — flag any undocumented drift either way.

## Step 3 — Fetch the target upstream release

```bash
git clone --depth 1 --branch iocraft-v<NEW_VERSION> https://github.com/ccbrown/iocraft.git upstream-target
```

Read the release notes for the target tag (`https://github.com/ccbrown/iocraft/releases/tag/iocraft-v<NEW_VERSION>`) and the changelog/commit log between the two tags:

```bash
git -C upstream-target log --oneline iocraft-v<OLD_VERSION>..iocraft-v<NEW_VERSION> -- iocraft
```

Summarize upstream changes before touching any code: bugfixes, breaking API changes, new deprecations, MSRV bumps. Flag anything that looks like it touches the same area as elph's customizations from Step 2.

## Step 4 — Compute the upstream delta

```bash
diff -ruN upstream-baseline/iocraft upstream-target/iocraft > /tmp/upstream-delta.diff
```

This is what changed *upstream only*, with no elph customization noise.

## Step 5 — Conflict analysis

Cross-reference `/tmp/elph-iocraft-customizations.diff` (step 2) against `/tmp/upstream-delta.diff` (step 4), file by file:

- **No overlap** — upstream didn't touch a file/region elph customized -> safe to take upstream wholesale for that file.
- **Overlap, compatible** — both changed the file but in different regions/functions -> three-way merge should apply cleanly.
- **Overlap, conflicting** — same lines/logic touched by both -> needs manual reconciliation. Read both intents before deciding.
- **Customization made obsolete** — upstream now implements what the elph patch was working around -> candidate for dropping the patch, but confirm with the user before removing it (ask-before-remove).

Write this classification out explicitly (a short table is fine) before editing anything — this is the actual "safety" of the upgrade.

## Step 6 — Apply the merge

For each file in `vendor/iocraft`, three-way merge using the baseline as common ancestor:

```bash
cp upstream-target/iocraft/src/<file> /tmp/theirs
cp upstream-baseline/iocraft/src/<file> /tmp/base
cp <elph-repo>/vendor/iocraft/src/<file> /tmp/ours

git merge-file /tmp/ours /tmp/base /tmp/theirs
# /tmp/ours now has merge conflict markers if any; resolve by hand
cp /tmp/ours <elph-repo>/vendor/iocraft/src/<file>
```

For files with no elph customization (per Step 5), just copy the upstream-target version directly. For new files upstream added, add them. For files upstream removed, check first whether elph's customization lives there — if so, ask before deleting.

Resolve every conflict marker manually. Do not auto-resolve by blindly preferring "theirs" or "ours" — that's exactly the failure mode this skill exists to prevent.

## Step 7 — Update version metadata

- Bump the version string in `vendor/iocraft/Cargo.toml`.
- Update or create `vendor/iocraft/VENDOR.md` with: upstream repo, new baseline tag/commit, date, and the current (possibly updated) list of customizations with a one-line reason each.
- If any customization was dropped in Step 6 (made obsolete), record that removal explicitly with the reasoning.
- Update `Cargo.lock` at the workspace root (`cargo update -p iocraft` or equivalent, scoped to the vendored path dependency).

## Step 8 — Verify (gate-green)

Run in this order, stop at first failure:

```bash
cargo check --workspace
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Then manually smoke-test elph's TUI (`cargo run -p elph` or the relevant binary) exercising whatever parts of the UI depend on the customized iocraft code paths — layout/render, input handling, whatever the customization touched. Given elph's known history with TUI measure/paint divergence, pay extra attention to rendering correctness here, not just compile success.

If verification fails, do not paper over it — go back to Step 5/6 for the specific file, don't force a "fix" without re-reading upstream intent.

## Step 9 — Prepare for commit (do NOT commit or push)

Commit and push are done by the user only — never run `git commit` or `git push` as part of this skill, even if verification is fully green.

Instead, leave the working tree staged and ready, and propose commit messages for the user to use:

1. `chore(vendor): sync iocraft to v<NEW_VERSION> upstream` — the raw upstream delta applied.
2. `chore(vendor): reapply elph iocraft customizations` — the merge-back of elph's patches, referencing `VENDOR.md`.

If the history doesn't support splitting cleanly, propose one commit message instead, listing: old version, new version, files with manual conflict resolution, and whether any customization was dropped. Present the proposed message(s) and a summary of staged changes (`git status`/`git diff --stat`) and stop there.

## Rollback

Before Step 6, the branch is disposable — if analysis in Step 5 reveals the upgrade is riskier than expected (e.g. major conflicting rewrite of a customized subsystem), stop and report findings to the user instead of pushing through. `git checkout -- vendor/iocraft` or dropping the branch is always the safe exit.

## Output to the user

Always report, regardless of outcome:

- Old version -> new version.
- Upstream changes relevant to elph (from Step 3 summary).
- Which files needed manual conflict resolution and why.
- Any customization dropped or newly at-risk.
- Verification results (build/test/clippy/manual TUI check).
- Proposed commit message(s) from Step 9 — the user commits and pushes themselves.
