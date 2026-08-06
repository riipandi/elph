# Pi port gap — output shapes

Pick the shape that fits the content. **Default to timeline prose and tagged bullets** so reports stay easy to skim.
**Do not use markdown tables** for status, audits, or gap lists (convert to bullets/timeline instead). **English** for all section titles and body text.

---

## 1. Opening (always)

Lead with a tight, glanceable block — no dense paragraphs. If any `args=` were
applied or skipped, say so in one line at the bottom.

```markdown
## Summary

**Upstream:** `<commit>` on `main` (vX.Y.Z [+ Unreleased])
**Last audit:** `<date>` @ `<prev-commit>`

- Upstream gap — **N items**: P0 `x`, P1 `x`, P2 `x`
- Undocumented drift (main ahead of CHANGELOG) — **K items**
- Elph delta — **M modules** (not porting debt, see §3)
- Next priority — one line, the single most important next step

_Args: `<applied flags, or "defaults">`_
```

---

## 2. Upstream gap — changelog timeline

```markdown
## Upstream gap

### pi-ai (`packages/ai` → `crates/elph-ai`)

#### Unreleased

- **[Gap P1]** Short title
  Upstream: `packages/ai/...` — what changed (one sentence).
  Elph: missing / partial in `crates/elph-ai/...` — evidence (`rg` / path).

- **[Parity]** Title
  Pi ↔ elph: both paths; behavior aligned (note shape differences if any).

#### [0.80.6] — 2026-07-09

- **[Partial]** …
  Present in elph: …
  Still missing: …

#### Undocumented (main ahead of CHANGELOG)

- **[Undocumented → Gap P1]** Short title
  Diff: `packages/ai/src/...` — what changed, no matching CHANGELOG bullet (`git diff <last-audited>..HEAD`).
  Elph: missing / partial in `crates/elph-ai/...`.

### pi-agent (`packages/agent` → `crates/elph-agent`)

#### Unreleased

- …
```

Tags: `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[Undocumented]`, `[N/A]`.
`[Undocumented]` items still get classified against elph — write it as
`[Undocumented → Gap/Partial/Parity]` so the priority tag stays visible.

Skip pure chore/docs unless `depth=full` (default) — call that out in the summary.
If the Phase 2b diff came back clean (no undocumented drift), say so in one
line instead of an empty section: `No undocumented drift since <commit>.`

---

## 3. Elph implementation delta (required)

**Elph-specific** features — explain design differences, not only “present”.

```markdown
## Elph implementation delta

### MCP (`crates/elph-agent/src/tools/mcp/`)

**In pi:** no MCP client in pi-agent-core.

**In Elph:** registry + session pool, `mcp_{server}__{tool}`, policy/approval,
Streamable HTTP / SSE, OAuth + AES-256 `auth.json` (`enc:`), project merge
`~/.elph` + `.elph/mcp.json`, tool-output truncation.

**Implications:** product wiring in `crates/coding-agent/` CLI/runtime; unrelated to model
catalog regen; pi-agent drift rarely touches this area.

### Hyper / gateway providers (`crates/elph-ai`)

**In pi:** no Hyper / TokenRouter / OpenGateway / Kilo product catalogs as in Elph.

**In Elph:** gateway catalogs under `models/*.json` + factories in
`builtin_providers()`; `generate-models chat` (models.dev origin) **preserves**
gateway route ids and enriches from models.dev. Skill: `update-models`.
No manual “re-add after wipe” ritual unless the provider was dropped from
`provider_sources` / `builtin_providers`.

**Implications:** catalog data work is Elph-owned (not a pi port); runtime
gaps for these providers (compat flags, OAuth, tool-schema sanitize) stay in
`src/api/*` and auth modules.
```

Per feature: **In pi** → **In Elph** → **Implications** (port risk, maintenance, coupling).

For **catalog/model list** bullets in Upstream gap, prefer:

```markdown
- **[N/A]** New models / catalog regen in pi
  Data: covered by Elph models.dev pipeline (`/update-models`). Confirm via
  `generate-models chat` if a specific model id is still missing.
  Runtime still needed only if: missing factory, wrong adapter, auth, stream flag.
```

---

## 4. Parity and nuance

```markdown
## Parity and nuance

**Deferred tools** — [Parity] equivalent API surface; elph in `deferred_tools.rs`,
pi in `packages/ai/...`. OpenAI Completions still has no native tool search (same as pi).

**Session context** — [Partial] transforms/projectors exist; JSONL v3 custom
metadata only matters if coding-agent interop is required.
```

---

## 5. Cross-crate

```markdown
## Cross-crate

`ThinkingLevel::Max` (elph-ai) and `AgentThinkingLevel::Max` (elph-agent)
are aligned. `added_tool_names` flows tool result → loop → harness. No open
mismatch — or call out split-brain if present.
```

---

## 6. Port priorities

Each priority line should name **gap intent** + **Elph landing zone** (not a pi path to copy).

```markdown
## Port priorities

1. **[P1]** … — _intent:_ …; _Elph:_ `crates/elph-ai/src/…` — because …
2. **[P2]** … — _intent:_ …; _Elph:_ `crates/elph-agent/src/…` — watch until …
3. **Catalog-only** … — not a port; run `/update-models` if models.dev lags
```

---

## 7. Persist to docs (optional)

Append under a timeline heading (prefer over new tables):

```markdown
### YYYY-MM-DDTHH:MM:SSZ @ `<commit>` (vX.Y.Z)

**Upstream gap:** brief N items (priority tags).
**Elph delta:** modules audited.
**Notes:** …
```

If the existing doc still uses tables, leave them unless you rewrite that
section — then migrate touched rows into timeline bullets.
