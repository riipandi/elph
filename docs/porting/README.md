# Pi → Elph porting status

How far the Rust crates lag (or lead) the TypeScript upstream
[earendil-works/pi](https://github.com/earendil-works/pi).

**Readability:** these pages prefer short prose, bullets, and timeline entries.
Avoid packing status into wide tables.

## Documents

- **[pi-ai.md](./pi-ai.md)** — `@earendil-works/pi-ai` (`packages/ai`) → `crates/elph-ai`
- **[pi-agent.md](./pi-agent.md)** — `@earendil-works/pi-agent-core` (`packages/agent`) → `crates/elph-agent`
- **[pi-coding-agent.md](./pi-coding-agent.md)** — `@earendil-works/pi-coding-agent` (`packages/coding-agent`) → `elph/` (product CLI + TUI)

## Why these docs exist

Mainstream pi moves quickly (model catalogs, provider fixes, agent-loop behavior).
Each page records:

1. What upstream has (pi).
2. What the port has (elph).
3. Gaps in either direction — port debt vs intentional Elph extensions.

## Baseline

Last documented **2026-07-11T11:23:28Z** (18:23 WIB).

- **Upstream:** https://github.com/earendil-works/pi
- **Local clone (analysis):** `/Users/ariss/Developer/github.com/earendil-works/pi`
- **Snapshot commit:** `4c18610` (_docs: audit unreleased changelogs_)
- **Package version:** `0.80.6` (released 2026-07-09) + **Unreleased** on `main`
- **Mapping:** `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent`, `packages/coding-agent` → `elph/`
- **Last library implementation pass:** 2026-07-11 — Sprints 1–4 on `elph-ai` / `elph-agent`
- **Last product gap audit:** 2026-07-11T12:14:13Z — coding-agent vs `elph/` (docs only)

## Status tags

Use these inline in prose (not table cells):

- **[Parity]** — behavior/API on both sides (shape may differ by language)
- **[Partial]** — present in elph but incomplete vs mainstream
- **[Gap]** / _missing in elph_ — in pi; not yet in elph (port debt)
- **[Elph delta]** / _missing in pi_ — Elph-only extension
- **[N/A]** — platform-specific; do not port 1:1

## Suggested sync workflow

1. Update the local pi clone: `git pull` in the clone path.
2. Read upstream changelogs (`packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`).
3. Diff against the timeline / remaining sections in this folder (prose, not tables).
4. Port + regenerate catalogs when needed:

    ```bash
    cargo run -p elph-ai --bin generate-models -- chat \
      --catalog-dir /path/to/pi/packages/ai --skip-scripts
    # Then re-add Elph-only Hyper define_catalog + index entry if wiped.
    ```

5. Append a **Timeline** entry with ISO timestamp + pi commit/version (bullet prose).

Or run the project skill **`/pi-port-gap`** for a full dual-lens report (upstream gap + Elph implementation delta).

## Related

- [`crates/elph-ai/README.md`](../../crates/elph-ai/README.md)
- [`crates/elph-agent/README.md`](../../crates/elph-agent/README.md)
- [docs/README.md](../README.md)
