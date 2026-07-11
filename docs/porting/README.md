# Pi → Elph porting status

Tracking documents for how far the Rust crates lag (or lead) the TypeScript upstream [earendil-works/pi](https://github.com/earendil-works/pi).

| Document                                   | Upstream package                                            | Elph crate                  |
| ------------------------------------------ | ----------------------------------------------------------- | --------------------------- |
| [pi-ai.md](./pi-ai.md)                     | `@earendil-works/pi-ai` (`packages/ai`)                     | `crates/elph-ai`            |
| [pi-agent.md](./pi-agent.md)               | `@earendil-works/pi-agent-core` (`packages/agent`)          | `crates/elph-agent`         |
| [pi-coding-agent.md](./pi-coding-agent.md) | `@earendil-works/pi-coding-agent` (`packages/coding-agent`) | `elph/` (product CLI + TUI) |

## Why these docs exist

Mainstream pi moves quickly (model catalogs, provider fixes, agent-loop behavior). These pages record:

1. **What upstream has** (pi).
2. **What the port has** (elph).
3. **Gaps** (in pi only, or in elph only / intentional extensions).

## Baseline snapshot

| Field                        | Value                                                                                         |
| ---------------------------- | --------------------------------------------------------------------------------------------- |
| **Documented at**            | 2026-07-11T11:23:28Z                                                                          |
| **Local timezone note**      | 2026-07-11 18:23 WIB                                                                          |
| **Upstream repo**            | https://github.com/earendil-works/pi                                                          |
| **Local clone (analysis)**   | `/Users/ariss/Developer/github.com/earendil-works/pi`                                         |
| **Upstream commit**          | `4c18610` (`docs: audit unreleased changelogs`)                                               |
| **Upstream package version** | `0.80.6` (released 2026-07-09) + **Unreleased** on `main`                                     |
| **Mapping**                  | `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent`, `packages/coding-agent` → `elph/` |
| **Last implementation pass** | 2026-07-11 — library Sprints 1–4 landed (`elph-ai` / `elph-agent`)                            |
| **Last product gap audit**   | 2026-07-11T12:14:13Z — `pi-coding-agent` vs `elph/` (docs only)                               |

## Status legend

| Status              | Meaning                                                           |
| ------------------- | ----------------------------------------------------------------- |
| **Parity**          | Behavior/API present on both sides (shape may differ by language) |
| **Partial**         | Present in elph but incomplete vs mainstream                      |
| **Missing in elph** | In pi; not (yet) in elph — **port gap**                           |
| **Missing in pi**   | In elph only — **Elph extension**                                 |
| **N/A**             | Platform-specific; do not 1:1 port                                |

## Suggested sync workflow

1. Update the local pi clone: `git pull` in the clone path.
2. Read upstream changelogs (`packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`).
3. Diff against gap tables in this folder.
4. Port + regenerate catalogs:

    ```bash
    cargo run -p elph-ai --bin generate-models -- chat \
      --catalog-dir /path/to/pi/packages/ai --skip-scripts
    # Then re-add Elph-only Hyper define_catalog + index entry if wiped.
    ```

5. Append an **Audit log** row with ISO timestamp + pi commit/version.

## Related

- [`crates/elph-ai/README.md`](../../crates/elph-ai/README.md)
- [`crates/elph-agent/README.md`](../../crates/elph-agent/README.md)
- [docs/README.md](../README.md)
