---
name: pi-port-gap
description: >-
    Analyze pi → elph porting gaps and Elph-specific implementation differences.
    Compare upstream pi-ai / pi-agent-core CHANGELOGs and source against elph-ai /
    elph-agent: (1) what upstream has that elph still lacks, (2) how Elph-only
    features diverge in design and wiring. Porting doctrine: adopt only true
    gaps (intent/behavior from pi), implement on Elph architecture — catalogs
    are models.dev via update-models, never pi seed. Prefer reverse-chronological
    timeline prose over tables. Persisted docs are always English; in-chat
    reports follow the user's current language. Use for port gap audit, upstream
    drift, parity check, Elph extension diff, implementation delta, changelog
    walk, selisih implementasi, or /pi-port-gap.
---

# Pi Port Gap Analysis

## Language

Split by destination:

- **Persisted docs** (`docs/porting/*.md` updates, Phase 5) — **always English**, no exceptions. Keep paths, commits, symbols, and upstream package names literal.
- **In-chat report** (Phase 6 deliverable, printed directly in the conversation) — **match the language the user is currently using** in the prompt (Indonesian, English, etc). Keep paths, commits, symbols, upstream package names, and code/technical identifiers literal/English regardless of chat language (e.g. _behavior_, _serialize_, _catalog_ stay as-is).
- If the user explicitly asks for a specific language for either destination, that overrides the default above.

## Arguments (optional)

Free-form flags appended after `/pi-port-gap`. Parse before Phase 1; apply
overrides; state any skipped/unrecognized flag in the Summary.

- `path=<dir>` — override upstream pi clone path (default: `/Users/ariss/Developer/github.com/earendil-works/pi`)
- `branch=<name>` — upstream ref to check out (default: `main` — always live HEAD, not a tag)
- `since=<commit|date>` — limit walk to changes after this point (default: last audit commit from `docs/porting/*.md`)
- `scope=ai|agent|coding-agent|all` — restrict crate scope (default: `ai,agent`)
- `module=<name>` — narrow Phase 3 to one Elph extension (e.g. `module=mcp`)
- `depth=full|quick` — `full` = chore/docs bullets + full source-level diff (Phase 2b); `quick` skips both (default: `full`)
- `persist=yes|no` — run Phase 5 without asking first (default: `no`, ask)
- `lang=id|en` — override in-chat report language (default: user's current chat language)

## Goal

Answer two questions in every run:

1. **Upstream gap** — What does mainstream pi already ship (or just release) that elph **still lacks** or only has **partially**?
2. **Elph implementation delta** — Features **built for Elph** (absent in pi, or designed differently): where the code lives, how it is wired, and what that means for maintenance and future porting.

Not an empty checklist. Deliver **changelog-style drift** plus **design/implementation differences** that support prioritization.

**Default scope:** `@earendil-works/pi-ai` → `crates/elph-ai`, `@earendil-works/pi-agent-core` → `crates/elph-agent`.
**Expand only if asked:** `pi-coding-agent` → `crates/coding-agent`.

### Porting doctrine (mandatory)

Elph is a **port of intent**, not a line-by-line TypeScript rewrite. When
**analyzing** gaps _or later implementing_ them (only if the user asks to port):

1. **Adopt the gap, not the pi shape** — Take the _behavior_, protocol, flag,
   type, test intent, or error-handling rule from pi. Do **not** copy pi’s file
   layout, package graph, TypeScript catalog scripts, or generator assumptions.
2. **Implement on Elph architecture** — Wire into existing elph-ai / elph-agent
   modules, factories, auth, and product surfaces (`crates/coding-agent/` only when
   product-facing). Prefer current Elph patterns (Rust modules, `builtin_providers`,
   harness/runtime split, MCP under `tools/mcp/`, etc.) over reintroducing
   pi-only paths.
3. **Catalog / models are Elph-owned** — Chat model catalogs are **not**
   generated from pi. Origin is **[models.dev](https://models.dev)** via
   `cargo run -p elph-ai --bin generate-models -- chat` (skill
   **`update-models`**). See **Architecture invariants** below.
4. **Classify correctly** — pi catalog/script changes that Elph already covers
   via models.dev + `generate-models` are usually **`[N/A]`** or
   **parity-by-other-means**, not “re-run pi generate-models”. Gaps remain only
   where Elph still lacks the _runtime_ behavior (API adapter, auth, stream
   flag, tool schema, agent-loop hook, etc.).
5. **Only true gaps get ported** — If Elph already has equivalent behavior under
   a different name/module, mark **[Parity]** / **[Partial]** with a nuance note;
   do not open a second implementation path “because pi looks different”.

#### Implementation checklist (when user explicitly asks to port)

Use this after a gap audit, not during a read-only `/pi-port-gap` run:

| Step               | Do                                                                                                                                                       |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Scope           | Port only the agreed gap(s). No drive-by refactors, no “while we’re here” catalog rewrite.                                                               |
| 2. Map             | Name the Elph target module(s) first (`src/api/…`, `builtin.rs`, harness, tools). Read existing neighbors before adding files.                           |
| 3. Shape           | Translate types/flags into Rust idioms already used in the crate (enums, `Option`, error types). Do not invent parallel TS-shaped APIs.                  |
| 4. Catalog data    | If the gap is “new model / pricing / context window” → **`/update-models`** or `generate-models chat`. Never seed from `packages/ai/src/providers/data`. |
| 5. Runtime gap     | Adapter/auth/stream/tool/loop changes go in Elph source; add/adjust unit tests next to the code.                                                         |
| 6. Product surface | Only touch `crates/coding-agent/` when the gap is user-visible (picker, slash, TUI key). Keep library behavior in crates.                                |
| 7. Verify          | `cargo test -p elph-ai` / `elph-agent` for touched crates; catalog registration test if providers changed.                                               |
| 8. Docs            | Significant behavior → update `docs/` (and timeline in `docs/porting/*` if this was a port pass). English for persisted docs.                            |

**Out of scope unless user asks:** runtime merge of `~/.elph/providers` JSON (schema exists; merge not required), reintroducing `--from-pi` / `--catalog-dir` for chat, dual catalog SSOT.

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

**Inline tags:** `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[Elph delta]`, `[Undocumented]`, `[N/A]`.

---

## Source of truth (read in this order)

1. Local baseline: [`docs/porting/README.md`](../../../docs/porting/README.md), [`pi-ai.md`](../../../docs/porting/pi-ai.md), [`pi-agent.md`](../../../docs/porting/pi-agent.md)
2. Upstream clone (default path, or `path=` override): `/Users/ariss/Developer/github.com/earendil-works/pi`
    - Track `main` (or `branch=` override) as live HEAD — **CHANGELOG lags reality**, never treat the last tag/CHANGELOG entry as the full picture.
    - `packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md` for the documented trail
    - `packages/ai/src/`, `packages/agent/src/` for the actual current implementation (source is the ground truth, CHANGELOG is the index)
3. Elph: `crates/elph-ai/`, `crates/elph-agent/` (+ public API in `src/lib.rs`)
4. **Elph catalog / generator (post models.dev cutover)** — not pi’s `packages/ai` data scripts:
    - `crates/elph-ai/bin/generate_models/` (`models_dev`, `provider_sources`, `normalize`, `thinking_map`, `pricing`, `chat`)
    - `crates/elph-ai/models/*.json`, `build.rs`, `src/models/catalog.rs`, `src/providers/builtin.rs`
    - Skill [`update-models`](../update-models/SKILL.md); schema contract [`schemas/provider-schema.json`](../../../schemas/provider-schema.json)
5. Extension scan hints: [`references/elph-extensions.md`](references/elph-extensions.md)
6. Output shapes: [`references/report-template.md`](references/report-template.md)
7. If clone missing: DeepWiki / GitHub `earendil-works/pi` for CHANGELOG + structure

### Architecture invariants (do not regress when porting)

These are **settled Elph design**. Porting must **not** reintroduce pi-centric alternatives without an explicit user decision.

| Area                              | Elph rule                                                                                                                                           |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chat model catalogs               | Origin = **models.dev** (`api.json`). **No** pi clone / `--from-pi` / npm `generate-models` for chat.                                               |
| Regenerating catalogs             | `generate-models chat` or `/update-models`. Every model has full **`thinkingLevelMap`** (7 keys).                                                   |
| Provider registration             | Every catalog provider id must exist in `builtin_providers()`; generator verifies this.                                                             |
| Pricing enrichment                | Live provider API when available → models.dev → previous non-zero.                                                                                  |
| Elph-only / gateway catalogs      | Preserve model **ids** (gateway routes); enrich from models.dev by id/fuzzy match. Do not wipe Hyper, Kilo, TokenRouter, OpenGateway, Sumopod, etc. |
| Wire `api` / `baseUrl` / `compat` | Owned by Elph factory + overlays — **not** invented from models.dev alone.                                                                          |
| Thinking UI / API                 | `get_supported_thinking_levels` + `map_thinking_level_for_api`; Ctrl+. / footer clamp to catalog maps.                                              |
| User provider JSON                | Schema prepared in `schemas/provider-schema.json`; **runtime override merge not required** unless user asks.                                        |
| OpenAI-compat gateways            | Non-standard detection in `src/api/openai_compat.rs` (no `store` / `developer` / etc. by default).                                                  |

**When pi CHANGELOG says “regenerated models” / “new model X”:**

- Prefer: run Elph **`update-models`** / `generate-models chat` and confirm models.dev already has the model; add/adjust `provider_sources` or overlays if needed.
- Only treat as a **runtime gap** if Elph still cannot call the model (missing factory, wrong API adapter, auth, stream flag, tool schema).
- Do **not** instruct agents to re-seed from `packages/ai/src/providers/data/*.json`.

---

## Workflow

### Phase 1 — Baseline

1. Resolve pi path (`path=` override or default). `git fetch origin <branch>` if network is available, checkout/pull `main` (or `branch=` override) — always work off live HEAD, not whatever the clone happened to have checked out. `git log -1 --oneline` and note dirty state.
2. Versions: `packages/ai/package.json`, `packages/agent/package.json` (+ Unreleased section if present) — a version label, not the source of truth.
3. Skim both CHANGELOGs: **Unreleased → recent tags**, newest first.
4. Read last-audited commit / notes from `docs/porting/*.md` (or `since=` override).
5. One-sentence baseline in the report: _pi @ `<commit>` on `main` (vX.Y.Z [+Unreleased]) vs last audit @ `<prev-commit>`_.

### Phase 2 — Upstream gap (CHANGELOG → code)

Drive from **upstream CHANGELOG bullets**, not from elph first.

For each material bullet (skip pure docs/chore noise unless the user wants a full walk):

1. **Locate in pi** — path + export + behavior in one sentence.
2. **Locate in elph** — `rg` / module map; note absence explicitly.
3. **Classify** — `[Parity]` | `[Partial]` | `[Gap Pn]` | `[N/A]`.
4. For Partial/Gap — state the **concrete missing piece** (type, hook, flag, provider branch, test), not a vague “not implemented”.

**elph-ai map:** `src/types/`, `src/api/`, `src/providers/` (incl. `builtin.rs`), `src/auth/`, `models/` + `bin/generate_models/`, `src/utils/` (tool_schema, deferred_tools, diagnostics, estimate), `src/session_resources.rs`

**elph-agent map:** `src/agent/` (incl. `harness/`, `subagent/`), `src/runtime/` (engine loop + env + proxy), `src/tools/` (incl. `mcp/`), `src/types/` (global enums), `src/collaboration/`, `src/session/`, `src/compaction/`, `src/messages/`, `src/prompt/encoding/`
(product modules belong under Phase 3 — not “gaps”)

#### Catalog / provider bullets in pi CHANGELOG

For each models/catalog/provider-list bullet:

1. Decide whether it is **data** (model list, pricing, context windows) vs **runtime** (new API surface, auth, stream quirk).
2. **Data** → map to Elph’s models.dev pipeline (`provider_sources`, overlays, `/update-models`). Usually **not** a hand-port of pi JSON.
3. **Runtime** → port into `src/api/*`, `builtin.rs` factory, auth/oauth, compat flags — following existing Elph patterns.
4. After catalog work (if any):

```sh
# Elph catalog origin — NOT pi
cargo run -p elph-ai --bin generate-models -- chat
# or offline after a prior fetch:
cargo run -p elph-ai --bin generate-models -- chat --offline --no-live-pricing
# verify registration + load
cargo test -p elph-ai --test providers catalog_providers_match_builtin_providers
```

Do **not** use obsolete flags/paths (`--catalog-dir`, pi `packages/ai` npm generate for chat). Gateways/Elph-only providers are preserved by the generator; no manual “re-add Hyper” ritual unless the provider was dropped from `provider_sources` or `builtin_providers`.

Priority heuristic when tagging gaps:

- **P0** — correctness / security / broken streams
- **P1** — user-visible provider or agent-loop behavior
- **P2** — polish, edge tests, optional interop

### Phase 2b — Source-level drift beyond CHANGELOG (always, skip only if `depth=quick`)

CHANGELOG entries are curated after the fact — `main` moves faster than the doc.
This phase catches real code changes that Phase 2 would miss entirely.

1. Diff pi source directly against the last-audited state: `git diff <last-audited-commit>..HEAD -- packages/ai/src packages/agent/src` (or `git log <last-audited-commit>..HEAD --oneline -- packages/ai/src packages/agent/src` if a full diff is too noisy).
2. Flag any structural change with no matching CHANGELOG bullet — new export, renamed/removed type, changed function signature, new provider branch, new tool, altered default — tag `[Undocumented]`.
3. Cross-check each `[Undocumented]` item against elph exactly like a normal CHANGELOG bullet (Phase 2 steps 2–4): locate in elph, classify `[Gap Pn]` / `[Partial]` / `[Parity]` / `[N/A]`, state the concrete missing piece.
4. Don't invent drift — if the diff is empty or purely mechanical (formatting, comment-only, dep bump with no behavior change), say so in one line and move on.

### Phase 3 — Elph implementation delta (always, independent of CHANGELOG)

Scan what Elph has that pi does **not** (or solves differently):

1. Start from [`references/elph-extensions.md`](references/elph-extensions.md); verify dirs vs pi packages.
2. Cross-check `crates/elph-agent/src/lib.rs` / `elph-ai` public surface and top-level `src/` modules absent upstream.
3. For **each** relevant extension, write **In pi / In Elph / Implications**:
    - **In pi** — absent, or nearest analogue
    - **In Elph** — modules, entry points, config/env, how it hooks the agent loop / CLI
    - **Implications** — maintenance burden, risk if upstream later ships something similar, coupling (elph CLI, downstream apps, MCP, Turso, …)

Do **not** collapse extensions into a single “[Elph-only]” bullet. The goal is **implementation difference**, not a status badge.

Depth targets when present: MCP (+ auth/crypto), goals, subagent, plugins, built-in tools, mode/plan, sandbox, datastore/Turso, TOON `prompt_encoding`, Hyper/Kilo/TokenRouter/OpenGateway (gateway stack), models.dev catalog generator, thinkingLevelMap + Ctrl+. clamp, skills, harness extras.

### Phase 4 — Cross-crate and parity nuance

- Features that span both crates must stay aligned (e.g. `Max` thinking, `added_tool_names`, deferred tools, estimate timestamp gate). Call out **split-brain** (one crate ported, the other not).
- **Same behavior, different shape** → `## Parity and nuance` (not a gap).

### Phase 5 — Persist docs (only if the user asks)

Append under a timeline heading in `docs/porting/pi-ai.md` / `pi-agent.md` (see report template). Update the baseline paragraph in `docs/porting/README.md` if the upstream commit advanced. Prefer prose timeline entries over new table rows. **Always English**, regardless of the language the conversation was conducted in.

### Phase 6 — Deliverable order

Always ship in this order in-chat, in the user's current language (paths/commits/symbols stay literal):

1. **Summary** — gap counts by priority, undocumented-drift count, headline Elph deltas, top next step
2. **Upstream gap** — CHANGELOG timeline + `[Undocumented]` source-level drift (`pi-ai`, then `pi-agent`)
3. **Elph implementation delta** — In pi / In Elph / Implications per module
4. **Parity and nuance**
5. **Cross-crate**
6. **Port priorities** — numbered; each item = **intent from pi** + **Elph landing zone** (crate path/module). Catalog-only items point to `/update-models`, not “copy pi JSON”.
7. If the user asks to implement next — follow **Implementation checklist** under Porting doctrine; stay read-only otherwise.

### Cross-skill handoff

| Need                                                             | Skill / command                                                      |
| ---------------------------------------------------------------- | -------------------------------------------------------------------- |
| Refresh model lists, pricing, thinkingLevelMap (`models/*.json`) | **`update-models`** / `generate-models chat`                         |
| Gap audit vs pi (this skill)                                     | **`pi-port-gap`** — read-only by default                             |
| Implement a runtime gap after audit                              | Explicit user ask → map to Elph modules; do not re-open catalog SSOT |
| Build quality after a port                                       | **`rust-verify-harden`**                                             |

---

## Commands (typical)

```sh
cd /path/to/pi && git log -1 --oneline && git status -sb
rg -n "^## |^- " packages/ai/CHANGELOG.md packages/agent/CHANGELOG.md | head -80
rg -n "pub mod" crates/elph-agent/src/lib.rs crates/elph-ai/src/lib.rs
# optional smoke after reading code
cargo test -p elph-ai --lib
cargo test -p elph-agent --lib
```

---

## Rules

- **Persisted docs always English**; **in-chat reports follow the user's current chat language**.
- **Two lenses always** — upstream gap **and** Elph implementation delta; never only one.
- **Timeline-first** — changelog walk is the spine of the gap section.
- **Readable reports** — short sections, tagged bullets; no status/audit/gap tables.
- **Tables minimized** — prose/timeline by default; table only if clearly denser and intentional (almost never for this skill). Architecture invariant tables in this skill file are for agents, not for report output.
- **Evidence** — path, symbol, or changelog line per claim.
- **Gap ≠ Elph extension** — gaps are pi→elph debt; extensions get design/implementation analysis.
- **Adopt gap intent, implement Elph shape** — when recommending or doing a port, map to Elph modules/architecture; do not reintroduce pi catalog generators or dual SSOT for models.
- **Only real gaps** — skip items Elph already covers under a different shape; document as Parity/nuance instead of inventing a second path.
- **Catalog SSOT = models.dev (+ Elph overlays)** — never direct agents to regenerate chat catalogs from pi data scripts. Obsolete: `--catalog-dir`, `--from-pi`, “re-add Hyper after generate-models”.
- **Port priorities name Elph landing zones** — never “copy `packages/ai/src/...` into elph”.
- **Read-only** on the pi clone unless the user asks to port.
- **No drive-by ports** unless the user explicitly asks to implement; then use the Implementation checklist.
- **Be honest about Partial** — better than false Parity.
- **`main` HEAD, not just tags/CHANGELOG** — CHANGELOG is curated after the fact; Phase 2b's direct source diff is what catches drift the doc hasn't caught up to yet.
- **Args override defaults, never scope** — `path=`/`branch=`/`since=`/`scope=`/`module=`/`depth=`/`persist=`/`lang=` adjust _how_ the run happens; they never skip Phase 3 (Elph delta) or the two-lenses rule.
