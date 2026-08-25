---
name: publish-acp-registry
description: >-
    Publish or update the Elph entry in the ACP Registry (riipandi/acp-registry).
    Clones the registry repo to a temporary directory, syncs elph/agent.json from
    crates/coding-agent/agent.json (version, archive URLs, sha256 from the latest
    GitHub release), runs auth validation (verify_agents.py --auth-check) and local
    schema/build validation (build_registry.py with SKIP_URL_VALIDATION=1), then
    commits and pushes the registry repo directly — never opens a PR.
    Use when publishing an ACP registry release, bumping the Elph registry version,
    updating elph/agent.json, or running registry validation checks.
metadata:
    scope: project
---

# Publish ACP Registry

## Language & Conventions

- In-chat reports follow the user's language.
- Persisted docs, skill text, and generated comments stay English.

## Overview

Elph is distributed as an ACP agent through the ACP Registry. The registry entry lives at
`elph/agent.json` in the `riipandi/acp-registry` repository. The canonical source of truth is
`crates/coding-agent/agent.json` in the Elph repo — the registry copy must stay in sync with it.

This skill updates the registry entry to the **latest GitHub release** of Elph, validates it,
and pushes the registry repo directly (no PR).

## Prerequisites

- A GitHub release of Elph exists with binary assets + a `SHA256SUMS` file (produced by the release pipeline).
- The registry repo `riipandi/acp-registry` is reachable over SSH (`git@github.com:riipandi/acp-registry.git`).
- `python3` with `jsonschema` available for `build_registry.py` (or `uv` to run it).
- Local clone of `riipandi/acp-registry` at `~/Developer/github.com/riipandi/acp-registry` (optional; the skill clones fresh when missing).

## Step-by-Step Execution Flow

### Step 1: Resolve the Latest Release

Query the GitHub API for the latest non-prerelease release of `riipandi/elph`:

```sh
curl -s -A "elph-acp-audit" https://api.github.com/repos/riipandi/elph/releases/latest
```

Notes:

- `releases/latest` returns the newest **non-prerelease** tag. Pre-releases (`-canary`) are excluded.
- Record the tag (e.g. `v0.1.0`) and the asset names it publishes.
- Fetch the checksums for that tag:

```sh
curl -sL -A "elph-acp-audit" \
  "https://github.com/riipandi/elph/releases/download/<TAG>/SHA256SUMS"
```

### Step 2: Clone the Registry Repo

Clone `riipandi/acp-registry` into a temporary directory (never work inside the Elph repo):

```sh
TMP=$(mktemp -d)
git clone git@github.com:riipandi/acp-registry.git "$TMP/acp-registry"
cd "$TMP/acp-registry"
```

If a local clone already exists at `~/Developer/github.com/riipandi/acp-registry`, it may be reused
after `git fetch origin && git reset --hard origin/main` to avoid stale state.

### Step 3: Update `elph/agent.json`

Copy the canonical manifest into the registry clone:

```sh
cp crates/coding-agent/agent.json "$TMP/acp-registry/elph/agent.json"
```

Then edit `elph/agent.json` so the `distribution.binary` entries match the resolved release:

- `version` must be strict `x.y.z` with numeric parts (matches the release tag without the `v`).
- Every `archive` URL must point at the resolved tag, e.g. `.../releases/download/v0.1.0/elph-...`.
- Every `sha256` must come from the release's `SHA256SUMS` file for the exact asset filename.
- No `/latest/` in any archive URL (registry validation rejects it).
- `cmd` and `args` stay as-is: `./elph` / `elph.exe` with `["acp", "--stdio"]`.

Registry platform keys → release asset mapping (keep in sync with `.github/workflows/release.yml`):

| Registry key     | Release asset               |
| ---------------- | --------------------------- |
| `darwin-aarch64` | `elph-macos-aarch64.tar.gz` |
| `darwin-x86_64`  | `elph-macos-x86_64.tar.gz`  |
| `linux-aarch64`  | `elph-linux-arm64.tar.gz`   |
| `linux-x86_64`   | `elph-linux-x86_64.tar.gz`  |
| `windows-x86_64` | `elph-windows-x86_64.zip`   |

### Step 4: Run Auth Validation

Verify the agent advertises ACP auth methods (`type: agent` or `type: terminal`):

```sh
python3 .github/workflows/verify_agents.py --auth-check --agent elph
```

This launches the agent in a sandbox, performs an ACP handshake, and checks the
`initialize` response advertises `authMethods`. Requires the release binary to be
downloadable (it is fetched from the `archive` URL).

### Step 5: Run Local Schema/Build Validation

Validate `agent.json` against `agent.schema.json` and build the aggregated registry without
hitting the network for every URL:

```sh
uv run --with jsonschema .github/workflows/build_registry.py
```

- `SKIP_URL_VALIDATION=1` skips URL accessibility checks (fine for local runs).
- If `uv` is unavailable, `pip install jsonschema` then run the script with `python3`.

### Step 6: Commit and Push (No PR)

Commit the updated entry and push directly to `main`:

```sh
git add elph/agent.json
git commit -m "chore: update elph registry entry to v0.1.0"
git push origin main
```

**Never open a pull request.** This repo is pushed to directly.

## Verification Checklist

- [ ] `version` in `agent.json` matches the resolved release tag.
- [ ] All `archive` URLs point at the resolved tag (no `/latest/`).
- [ ] All `sha256` values match the release `SHA256SUMS` for the exact asset filenames.
- [ ] `verify_agents.py --auth-check --agent elph` passes.
- [ ] `uv run --with jsonschema .github/workflows/build_registry.py` passes.
- [ ] Commit pushed to `origin/main` (no PR opened).

## Strict Invariants & Prohibitions

- **Never create a PR** — push directly to `main`.
- **Never use `/latest/` in archive URLs.**
- **Never guess sha256** — always read from the release `SHA256SUMS`.
- **Never edit the registry `agent.json` by hand without syncing the canonical
  `crates/coding-agent/agent.json`** — keep both in sync.
- **Never work inside the Elph repo** for registry changes; use the temporary clone.
- **Do not commit without user confirmation** for the push step.
