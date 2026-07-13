---
name: pi-port-gap
description: >-
    Analyze pi → elph porting gaps and Elph-specific implementation differences.
    Compare upstream pi-ai / pi-agent-core CHANGELOGs and source against elph-ai /
    elph-agent: (1) what upstream has that elph still lacks, (2) how Elph-only
    features diverge in design and wiring. Prefer reverse-chronological timeline
    prose over tables. Write all skill output and docs in English.
    Use for port gap audit, upstream drift, parity check, Elph extension diff,
    implementation delta, changelog walk, selisih implementasi, or /pi-port-gap.
---

# Pi Port Gap Analysis

## Language

**Always write skill output, report sections, and any updates to `docs/porting/*` in English** (e.g. _behavior_, _serialize_, _catalog_).
Keep paths, commits, symbols, and upstream package names literal. Indonesian (or other) user prompts are fine as input; respond in English unless the user explicitly asks for another language.

## Goal

Answer two questions in every run:

1. **Upstream gap** — What does mainstream pi already ship (or just release) that elph **still lacks** or only has **partially**?
2. **Elph implementation delta** — Features **built for Elph** (absent in pi, or designed differently): where the code lives, how it is wired, and what that means for maintenance and future porting.

Not an empty checklist. Deliver **changelog-style drift** plus **design/implementation differences** that support prioritization.

**Default scope:** `@earendil-works/pi-ai` → `crates/elph-ai`, `@earendil-works/pi-agent-core` → `crates/elph-agent`.
**Expand only if asked:** `pi-coding-agent` → `elph/`.

---

## Smart formatting (readability first, timeline spine)

**Default medium is scannable prose:** short paragraphs, tagged bullets, and reverse-chronological changelog sections. Readers should not need to parse wide grids.

Pick shape by content (smart, not rigid):

- **Upstream drift** → `## Upstream gap` → `### pi-ai` / `### pi-agent` → `#### Unreleased` then `#### [version]`, newest first
- **One feature deep-dive** → **In pi** / **In Elph** / **Implications**
- **Many small gaps** → tagged bullets under the version heading (one idea per bullet)
- **Elph-only modules** → `## Elph implementation delta`, one `###` per module, three-part block
- **Cross-crate** → short paragraph or 2–3 bullets
- **Priorities** → numbered list + one-line _why_

**Tables — minimize hard:**

- Do **not** use tables for status matrices, audit logs, “at a glance”, implementation maps, or gap lists.
- Prefer: metadata as bold field lines, status as `- Topic — **[Tag]** detail`, history as `### timestamp @ commit` timeline entries.
- A table is allowed only if a compact multi-axis comparison is _genuinely_ clearer than bullets (rare). If you almost reach for a table, try bullets first.
- When **editing** `docs/porting/*.md`, convert any table you touch into prose/timeline; do not add new tables.

**Mermaid:** only if port ordering is hard to follow in prose.

**Inline tags:** `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[Elph delta]`, `[N/A]`.

---

## Source of truth (read in this order)

1. Local baseline: [`docs/porting/README.md`](../../../docs/porting/README.md), [`pi-ai.md`](../../../docs/porting/pi-ai.md), [`pi-agent.md`](../../../docs/porting/pi-agent.md)
2. Upstream clone (default): `/Users/ariss/Developer/github.com/earendil-works/pi`
    - `packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`
    - matching `packages/ai/src/`, `packages/agent/src/`
3. Elph: `crates/elph-ai/`, `crates/elph-agent/` (+ public API in `src/lib.rs`)
4. Extension scan hints: [`references/elph-extensions.md`](references/elph-extensions.md)
5. Output shapes: [`references/report-template.md`](references/report-template.md)
6. If clone missing: DeepWiki / GitHub `earendil-works/pi` for CHANGELOG + structure

---

## Workflow

### Phase 1 — Baseline

1. Resolve pi path (default above, or user override). `git fetch` if network is available; always `git log -1 --oneline` and note dirty state.
2. Versions: `packages/ai/package.json`, `packages/agent/package.json` (+ Unreleased section if present).
3. Skim both CHANGELOGs: **Unreleased → recent tags**, newest first.
4. Read last-audited commit / notes from `docs/porting/*.md`.
5. One-sentence baseline in the report: _pi @ commit (version) vs last audit @ …_.

### Phase 2 — Upstream gap (CHANGELOG → code)

Drive from **upstream CHANGELOG bullets**, not from elph first.

For each material bullet (skip pure docs/chore noise unless the user wants a full walk):

1. **Locate in pi** — path + export + behavior in one sentence.
2. **Locate in elph** — `rg` / module map; note absence explicitly.
3. **Classify** — `[Parity]` | `[Partial]` | `[Gap Pn]` | `[N/A]`.
4. For Partial/Gap — state the **concrete missing piece** (type, hook, flag, provider branch, test), not a vague “not implemented”.

**elph-ai map:** `src/types/`, `src/api/`, `src/providers/`, `src/auth/`, `models/`, `src/utils/deferred_tools.rs`, `src/utils/diagnostics.rs`, `src/utils/estimate.rs`, `src/session_resources.rs`

**elph-agent map:** `src/agent/`, `src/agent_loop/`, `src/types/`, `src/session/`, `src/compaction/`, `src/harness/`
(product modules belong under Phase 3 — not “gaps”)

After any model-catalog port work:

```bash
cargo run -p elph-ai --bin generate-models -- chat \
  --catalog-dir /path/to/pi/packages/ai --skip-scripts
# Re-add Hyper (Elph-only) if generate-models wiped it.
```

Priority heuristic when tagging gaps:

- **P0** — correctness / security / broken streams
- **P1** — user-visible provider or agent-loop behavior
- **P2** — polish, edge tests, optional interop

### Phase 3 — Elph implementation delta (always, independent of CHANGELOG)

Scan what Elph has that pi does **not** (or solves differently):

1. Start from [`references/elph-extensions.md`](references/elph-extensions.md); verify dirs vs pi packages.
2. Cross-check `crates/elph-agent/src/lib.rs` / `elph-ai` public surface and top-level `src/` modules absent upstream.
3. For **each** relevant extension, write **In pi / In Elph / Implications**:
    - **In pi** — absent, or nearest analogue
    - **In Elph** — modules, entry points, config/env, how it hooks the agent loop / CLI
    - **Implications** — maintenance burden, risk if upstream later ships something similar, coupling (elph CLI, downstream apps, MCP, Turso, …)

Do **not** collapse extensions into a single “[Elph-only]” bullet. The goal is **implementation difference**, not a status badge.

Depth targets when present: MCP (+ auth/crypto), goals, subagent, plugins, built-in tools, mode/plan, sandbox, datastore/Turso, TOON `prompt_encoding`, Hyper, skills, harness extras.

### Phase 4 — Cross-crate and parity nuance

- Features that span both crates must stay aligned (e.g. `Max` thinking, `added_tool_names`, deferred tools, estimate timestamp gate). Call out **split-brain** (one crate ported, the other not).
- **Same behavior, different shape** → `## Parity and nuance` (not a gap).

### Phase 5 — Persist docs (only if the user asks)

Append under a timeline heading in `docs/porting/pi-ai.md` / `pi-agent.md` (see report template). Update the baseline paragraph in `docs/porting/README.md` if the upstream commit advanced. Prefer prose timeline entries over new table rows. Use English in all written docs.

### Phase 6 — Deliverable order

Always ship in this order (English headings; keep paths/commits literal):

1. **Summary** — gap counts by priority, headline Elph deltas, top next step
2. **Upstream gap** — CHANGELOG timeline (`pi-ai`, then `pi-agent`)
3. **Elph implementation delta** — In pi / In Elph / Implications per module
4. **Parity and nuance**
5. **Cross-crate**
6. **Port priorities** — numbered

---

## Commands (typical)

```bash
cd /path/to/pi && git log -1 --oneline && git status -sb
rg -n "^## |^- " packages/ai/CHANGELOG.md packages/agent/CHANGELOG.md | head -80
rg -n "pub mod" crates/elph-agent/src/lib.rs crates/elph-ai/src/lib.rs
# optional smoke after reading code
cargo test -p elph-ai --lib
cargo test -p elph-agent --lib
```

---

## Rules

- **English** for all reports and doc edits produced by this skill.
- **Two lenses always** — upstream gap **and** Elph implementation delta; never only one.
- **Timeline-first** — changelog walk is the spine of the gap section.
- **Readable reports** — short sections, tagged bullets; no status/audit/gap tables.
- **Tables minimized** — prose/timeline by default; table only if clearly denser and intentional (almost never for this skill).
- **Evidence** — path, symbol, or changelog line per claim.
- **Gap ≠ Elph extension** — gaps are pi→elph debt; extensions get design/implementation analysis.
- **Read-only** on the pi clone unless the user asks to port.
- **No drive-by ports** unless the user explicitly asks to implement.
- **Be honest about Partial** — better than false Parity.
