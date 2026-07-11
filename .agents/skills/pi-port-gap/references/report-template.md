# Pi port gap — output shapes

Pick the shape that fits the content. **Default to timeline prose and tagged bullets** so reports stay easy to skim.
**Do not use markdown tables** for status, audits, or gap lists (convert to bullets/timeline instead). **English** for all section titles and body text.

---

## 1. Opening (always)

```markdown
## Summary

Upstream pi @ `<commit>` (vX.Y.Z [+ Unreleased]). Previous audit: `<date>` @ `<prev-commit>`.

**Upstream gap:** N items — P0 …, P1 …, P2 … (missing or partial in elph).

**Elph delta:** M Elph-specific modules — not porting debt; see implementation notes below.

Next priority: …
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

### pi-agent (`packages/agent` → `crates/elph-agent`)

#### Unreleased

- …
```

Tags: `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[N/A]`.

Skip pure chore/docs unless the user wants a full walk — call that out in the summary.

---

## 3. Elph implementation delta (required)

**Elph-specific** features — explain design differences, not only “present”.

```markdown
## Elph implementation delta

### MCP (`crates/elph-agent/src/mcp/`)

**In pi:** no MCP client in pi-agent-core.

**In Elph:** registry + session pool, `mcp_{server}__{tool}`, policy/approval,
Streamable HTTP / SSE, OAuth + AES-256 `auth.json` (`enc:`), project merge
`~/.elph` + `.elph/mcp.json`, tool-output truncation.

**Implications:** product wiring in `elph/` CLI/runtime; unrelated to model
catalog regen; pi-agent drift rarely touches this area.

### Hyper provider (`crates/elph-ai`)

**In pi:** no such provider.

**In Elph:** `models/hyper.json` + provider/OAuth; must re-add after
`generate-models` if wiped.

**Implications:** Elph-only unless upstream adds it; document on every
catalog regen step.
```

Per feature: **In pi** → **In Elph** → **Implications** (port risk, maintenance, coupling).

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

```markdown
## Port priorities

1. **[P1]** … — because …
2. **[P2]** … — watch until …
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
