# OpenWiki port gap — output shapes

**Default to timeline prose and tagged bullets** so reports stay easy to skim. **Do not use markdown tables** for status, audits, or gap lists. **American English** for all section titles and body text.

---

## 1. Opening (always)

```markdown
## Summary

Upstream OpenWiki @ `<commit>` (vX.Y.Z [+ Unreleased]). Previous audit: `<date>` @ `<prev-commit>`.

**Upstream gap:** N items — P0 …, P1 …, P2 … (missing or partial in owly).

**Owly delta:** M Owly/Elph-specific modules — not pure porting debt; see implementation notes below.

**Architecture:** code-mode focus on elph-agent (OpenWiki personal/connectors often out of scope unless noted).

Next priority: …
```

---

## 2. Upstream gap — changelog / feature timeline

```markdown
## Upstream gap

#### Unreleased / recent commits

- **[Gap P1]** Short title
  OpenWiki: `path/…` — what changed (one sentence).
  Owly: missing / partial in `owly/src/…` — evidence (`rg` / path).

- **[Parity]** Title
  OpenWiki ↔ owly: both paths; behavior aligned (note shape differences).

#### [version] — date

- **[Partial]** …
  Present in owly: …
  Still missing: …
```

Tags: `[Gap P0|P1|P2]`, `[Partial]`, `[Parity]`, `[N/A]`.

Group by **product area** when CHANGELOG is thin (CLI modes, update pipeline, connectors, auth, CI).

---

## 3. Owly implementation delta (required)

```markdown
## Owly implementation delta

### elph-agent runtime

**In OpenWiki:** LangChain / LangGraph agent graph, Node tools.

**In Owly:** `owly/src/agent/` on `elph-agent` + `elph-ai` models/providers.

**Implications:** feature parity is behavioral, not file-for-file; graph-native
OpenWiki features may need harness/tool redesign.

### Interactive TUI / shell

**In OpenWiki:** Node interactive CLI (Ink or similar as upstream evolves).

**In Owly:** `shell/`, `tui/` wired to elph-tui patterns.

**Implications:** UX drift is expected; measure by user journeys (init, update, chat).
```

Per feature: **In OpenWiki** → **In Owly** → **Implications**.

---

## 4. Architecture delta

```markdown
## Architecture delta

OpenWiki is a Node npm CLI with personal + code modes, connector ingestion under
`~/.openwiki/`, and LangGraph checkpointing. Owly is a Rust workspace crate aimed
at repository `openwiki/` generation on elph-agent, with config under `~/.owly/`.
Do not score “missing LangGraph” as a product gap unless a specific user-visible
behavior is missing.
```

---

## 5. Parity and nuance

```markdown
## Parity and nuance

**Code init/update** — [Partial] both maintain `openwiki/` wiki trees; Owly update
no-op and metadata live in `metadata.rs` / `.last-update.json` (confirm vs upstream).

**AGENTS.md / CLAUDE.md markers** — [Partial] OpenWiki maintains OPENWIKI blocks;
Owly ecosystem helpers may differ — cite paths.
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

Append under a timeline heading in `docs/porting/openwiki.md`:

```markdown
### YYYY-MM-DDTHH:MM:SSZ @ `<commit>` (vX.Y.Z)

**Upstream gap:** brief N items (priority tags).
**Owly delta:** modules audited.
**Notes:** …
```
