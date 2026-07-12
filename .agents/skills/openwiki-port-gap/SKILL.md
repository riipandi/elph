---
name: openwiki-port-gap
description: >-
    Analyze OpenWiki → owly porting gaps and Owly-specific implementation differences.
    Compare upstream langchain-ai/openwiki CHANGELOG and source against the owly crate:
    (1) what upstream has that owly still lacks, (2) how Owly-only features diverge
    (elph-agent runtime, TUI, checkpoint, providers). Prefer reverse-chronological
    timeline prose over tables. Write all skill output and docs in American English.
    Use for openwiki port gap audit, owly parity, upstream drift, wiki CLI port,
    implementation delta, or /openwiki-port-gap.
---

# OpenWiki Port Gap Analysis

## Language

**Always write skill output, report sections, and any updates to `docs/porting/*` in American English** (e.g. _behavior_, _serialize_, _catalog_).
Keep paths, commits, symbols, and upstream package names literal. Indonesian (or other) user prompts are fine as input; respond in American English unless the user explicitly asks for another language.

## Goal

Answer two questions in every run:

1. **Upstream gap** — What does mainstream [OpenWiki](https://github.com/langchain-ai/openwiki) already ship (or just release) that **owly** still lacks or only has **partially**?
2. **Owly implementation delta** — Features **built for Owly / Elph** (absent in OpenWiki, or designed differently): where the code lives, how it is wired, and what that means for maintenance and future porting.

Not an empty checklist. Deliver **changelog-style drift** plus **design/implementation differences** that support prioritization.

**Default scope:** `langchain-ai/openwiki` (TypeScript / npm CLI) → `owly/` (Rust crate).

**Important framing:**

- Upstream product modes: **code** (`openwiki/` in a repo) and **personal** (`~/.openwiki/wiki` + connectors). Owly today is primarily a **code-mode** port on **elph-agent** / **elph-ai**, not a full personal-brain + connector suite.
- Runtime is intentional: OpenWiki uses LangChain/LangGraph (+ Node checkpointing); Owly uses `elph-agent` / `elph-ai`. Treat runtime swap as **architecture delta**, not a failed 1:1 file port.

**Expand only if asked:** deep dive into generated wiki content quality, CI workflow examples, or personal-mode roadmap.

---

## Smart formatting (readability first, timeline spine)

**Default medium is scannable prose:** short paragraphs, tagged bullets, and reverse-chronological changelog sections. Readers should not need to parse wide grids.

Pick shape by content (smart, not rigid):

- **Upstream drift** → `## Upstream gap` → `#### Unreleased` then `#### [version]` / recent commits, newest first
- **One feature deep-dive** → **In OpenWiki** / **In Owly** / **Implications**
- **Many small gaps** → tagged bullets under a version or area heading
- **Owly-only modules** → `## Owly implementation delta`, one `###` per module, three-part block
- **Runtime / stack** → short paragraph under `## Architecture delta`
- **Priorities** → numbered list + one-line _why_

**Tables — minimize hard:**

- Do **not** use tables for status matrices, audit logs, “at a glance”, implementation maps, or gap lists.
- Prefer: metadata as bold field lines, status as `- Topic — **[Tag]** detail`, history as `### timestamp @ commit` timeline entries.
- A table is allowed only if a compact multi-axis comparison is _genuinely_ clearer than bullets (rare).
- When **editing** `docs/porting/openwiki.md` (or the porting index), convert any table you touch into prose/timeline; do not add new tables.

**Mermaid:** only if mode/runtime flows are hard to follow in prose.

**Inline tags:** `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[Owly delta]`, `[N/A]`.

---

## Source of truth (read in this order)

1. Local baseline: [`docs/porting/openwiki.md`](../../../docs/porting/openwiki.md), [`docs/porting/README.md`](../../../docs/porting/README.md)
2. Product notes: [`owly/README.md`](../../../owly/README.md), [`NOTICE.md`](../../../NOTICE.md) (license / port attribution)
3. Generated wiki contract (this monorepo): [`openwiki/quickstart.md`](../../../openwiki/quickstart.md), [`openwiki/architecture.md`](../../../openwiki/architecture.md)
4. Upstream clone (default): `/Users/ariss/Developer/github.com/langchain-ai/openwiki`
    - Fallback: clone or DeepWiki / GitHub `langchain-ai/openwiki`
    - Prefer `CHANGELOG.md` / `package.json` / `src/` (or package layout as published)
5. Owly: `owly/src/` (+ public surface in `owly/src/lib.rs`)
6. Extension scan hints: [`references/owly-extensions.md`](references/owly-extensions.md)
7. Output shapes: [`references/report-template.md`](references/report-template.md)

---

## Workflow

### Phase 1 — Baseline

1. Resolve OpenWiki path (default above, or user override). `git fetch` if network is available; always `git log -1 --oneline` and note dirty / behind state.
2. Version: `package.json` (and Unreleased / recent tags if changelog exists).
3. Skim CHANGELOG (or recent commits if no changelog): **newest first**.
4. Read last-audited notes from `docs/porting/openwiki.md`.
5. One-sentence baseline: _OpenWiki @ commit (version) vs last audit @ …; owly @ …_.

### Phase 2 — Upstream gap (CHANGELOG / features → code)

Drive from **upstream features and CHANGELOG bullets**, not from owly first.

For each material feature (skip pure docs/chore noise unless the user wants a full walk):

1. **Locate in OpenWiki** — path + export + behavior in one sentence.
2. **Locate in owly** — `rg` / module map; note absence explicitly.
3. **Classify** — `[Parity]` | `[Partial]` | `[Gap Pn]` | `[N/A]`.
4. For Partial/Gap — state the **concrete missing piece** (CLI flag, mode, connector, prompt, checkpoint API, CI workflow), not a vague “not implemented”.

**Classify carefully:**

- Features that only exist because of **LangGraph / Node / personal connectors** may be `[N/A]` or roadmap `[Gap P2]` depending on Owly goals—not automatic P0.
- **Code-mode** wiki generation, update no-op, frontmatter, AGENTS.md/CLAUDE.md markers, and interactive chat are the highest-signal parity surface.

**owly module map:**

- CLI / modes: `cli.rs`, `commands/`, `main.rs`, `startup.rs`
- Agent run: `agent/`, `prompts.rs`, `docs.rs`
- Config / providers: `config.rs`, `constants/`, `credentials.rs`, `env.rs`, `onboarding.rs`
- Session / checkpoint: `session/`, `checkpoint/`
- Docs pipeline: `frontmatter.rs`, `metadata.rs`, `ecosystem.rs`
- UX: `shell/`, `tui/`, `ask_user/`, `ui_events.rs`
- Support: `diagnostics.rs`, `utils.rs`

Priority heuristic when tagging gaps:

- **P0** — correctness of code-mode init/update, data loss, broken CLI entry
- **P1** — user-visible code-mode behavior (update no-op, markers, models, interactive UX)
- **P2** — polish, personal mode, connectors, CI templates, optional interop

### Phase 3 — Owly implementation delta (always, independent of CHANGELOG)

Scan what Owly has that OpenWiki does **not** (or solves differently):

1. Start from [`references/owly-extensions.md`](references/owly-extensions.md); verify against upstream tree.
2. Cross-check `owly/src/lib.rs` modules and Elph stack coupling (`elph-agent`, `elph-ai`, `elph-tui`).
3. For **each** relevant delta, write **In OpenWiki / In Owly / Implications**:
    - **In OpenWiki** — absent, or nearest analogue
    - **In Owly** — modules, entry points, config/env, how it hooks the agent loop / CLI
    - **Implications** — maintenance burden, risk if upstream later ships similar, coupling to Elph crates

Do **not** collapse deltas into a single “[Owly-only]” badge. The goal is **implementation difference**.

Depth targets when present: elph-agent runtime, elph-ai multi-provider catalog, TUI/shell host, checkpoint/session design, TOON/prompt encoding via agent stack, redaction helpers, explicit config (no hidden state), monorepo wiki conventions.

### Phase 4 — Architecture delta (runtime stack)

Always include a short **Architecture delta** section:

- **OpenWiki:** Node CLI, LangChain/LangGraph agent graph, SQLite checkpointing (`better-sqlite3`), npm global install, personal + code modes, connectors under `~/.openwiki/`.
- **Owly:** Rust crate in the Elph workspace, `elph-agent` / `elph-ai`, checkpoint modules under `owly/src/checkpoint/`, config under `~/.owly/`, primarily code-mode wiki in `openwiki/`.

Call out **same product intent, different stack** so gaps are not misread as “missing LangGraph”.

### Phase 5 — Persist docs (only if the user asks)

Append under a timeline heading in `docs/porting/openwiki.md` (see report template). Update the baseline paragraph in `docs/porting/README.md` if the upstream commit advanced. Prefer prose timeline entries. Use American English.

### Phase 6 — Deliverable order

Always ship in this order (American English headings; keep paths/commits literal):

1. **Summary** — gap counts by priority, headline Owly deltas, top next step
2. **Upstream gap** — CHANGELOG / feature timeline
3. **Owly implementation delta** — In OpenWiki / In Owly / Implications per module
4. **Architecture delta** — stack/runtime
5. **Parity and nuance**
6. **Port priorities** — numbered

---

## Commands (typical)

```bash
# Upstream
cd /path/to/openwiki && git log -1 --oneline && git status -sb
test -f CHANGELOG.md && rg -n "^## |^- " CHANGELOG.md | head -80
ls src packages apps 2>/dev/null; cat package.json | head -40

# Owly
rg -n "pub mod" owly/src/lib.rs
ls owly/src
rg -n "personal|connector|ingest|code --init|OPENWIKI" owly/src | head -40

# Optional smoke
cargo test -p owly
```

If the local clone is missing:

```bash
# Read-only remote (no commit)
git ls-remote https://github.com/langchain-ai/openwiki.git HEAD
# Or DeepWiki: langchain-ai/openwiki
```

---

## Rules

- **American English** for all reports and doc edits produced by this skill.
- **Two lenses always** — upstream gap **and** Owly implementation delta; never only one.
- **Timeline-first** — changelog (or commit) walk is the spine of the gap section.
- **Readable reports** — short sections, tagged bullets; no status/audit/gap tables.
- **Tables minimized** — prose/timeline by default.
- **Evidence** — path, symbol, CLI flag, or changelog line per claim.
- **Gap ≠ Owly extension** — gaps are OpenWiki→owly debt; extensions get design/implementation analysis.
- **Stack delta is not a bug** — LangGraph vs elph-agent is intentional unless the user asks for parity of a specific behavior.
- **Read-only** on the OpenWiki clone unless the user asks to port.
- **No drive-by ports** unless the user explicitly asks to implement.
- **Be honest about Partial** — better than false Parity.
