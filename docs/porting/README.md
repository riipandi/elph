# Pi → Elph porting status

Tracking documents for how far the Rust crates lag (or lead) the TypeScript
upstream [earendil-works/pi](https://github.com/earendil-works/pi).

| Document                     | Upstream package                                   | Elph crate          |
| ---------------------------- | -------------------------------------------------- | ------------------- |
| [pi-ai.md](./pi-ai.md)       | `@earendil-works/pi-ai` (`packages/ai`)            | `crates/elph-ai`    |
| [pi-agent.md](./pi-agent.md) | `@earendil-works/pi-agent-core` (`packages/agent`) | `crates/elph-agent` |

## Why these docs exist

Mainstream pi moves quickly (model catalogs, provider fixes, agent-loop
behavior). The port was close to **0.80.x** architecture but not a line-by-line
mirror. These pages record:

1. **What upstream has** (pi).
2. **What the port has** (elph).
3. **Gaps** (in pi only, or in elph only / intentional extensions).

Use them when syncing from a new pi tag or PR so nothing is lost silently.

## Baseline snapshot

| Field                        | Value                                                      |
| ---------------------------- | ---------------------------------------------------------- |
| **Documented at**            | 2026-07-11T11:12:19Z                                       |
| **Local timezone note**      | 2026-07-11 18:12 WIB                                       |
| **Upstream repo**            | https://github.com/earendil-works/pi                       |
| **Local clone (analysis)**   | `/Users/ariss/Developer/github.com/earendil-works/pi`      |
| **Upstream commit**          | `4c18610` (`docs: audit unreleased changelogs`)            |
| **Upstream package version** | `0.80.6` (released 2026-07-09) + **Unreleased** on `main`  |
| **Mapping**                  | `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent` |

> Re-audit after each intentional sync: update the table above, then the
> gap tables in each package doc. Prefer a new **Audit log** row rather than
> rewriting history.

## Status legend

| Status              | Meaning                                                           |
| ------------------- | ----------------------------------------------------------------- |
| **Parity**          | Behavior/API present on both sides (shape may differ by language) |
| **Partial**         | Present in elph but incomplete vs mainstream                      |
| **Missing in elph** | In pi; not (yet) in elph — **port gap**                           |
| **Missing in pi**   | In elph only — **Elph extension** (not expected upstream)         |
| **N/A**             | Platform-specific; do not 1:1 port (e.g. Node lazy modules)       |

## Priority guide

| Priority | When to act                                                                          |
| -------- | ------------------------------------------------------------------------------------ |
| **P0**   | Wrong cost, missing models users select, or broken Claude/OpenAI tool/thinking paths |
| **P1**   | Correctness fixes already landed upstream (empty thinking, Bedrock apiKey, estimate) |
| **P2**   | Pluggability / observability / session interop polish                                |
| **Skip** | JS bundler / emit concerns that do not apply to Rust                                 |

## Suggested sync workflow

1. Update the local pi clone: `git pull` in the clone path (or re-clone).
2. Read upstream changelogs:
    - `packages/ai/CHANGELOG.md`
    - `packages/agent/CHANGELOG.md`
3. Diff against the **Missing in elph** tables in this folder.
4. Port in this order when catching up:
    1. Types + catalogs (`ThinkingLevel`, `ModelCost.tiers`, model JSON)
    2. Provider correctness fixes
    3. Deferred tools / agent-loop propagation
    4. Harness session transforms / diagnostics
5. Append an **Audit log** entry with ISO timestamp + pi commit/version.
6. Mark closed gaps **Parity** (or **Partial** with a note).

## Related code docs

- Crate READMEs: [`crates/elph-ai/README.md`](../../crates/elph-ai/README.md), [`crates/elph-agent/README.md`](../../crates/elph-agent/README.md)
- Agent models design note: [`crates/elph-agent/docs/models.md`](../../crates/elph-agent/docs/models.md)
- Product design index: [docs/README.md](../README.md)
- Living implementation docs: [openwiki/quickstart.md](../../openwiki/quickstart.md)
