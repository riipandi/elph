# Porting status: pi-ai → elph-ai

**Last audited:** 2026-07-11T11:23:28Z
**Upstream:** `@earendil-works/pi-ai` · `packages/ai` · **v0.80.6** + Unreleased
**Upstream commit:** `4c18610`
**Elph crate:** `crates/elph-ai`

---

## At a glance (post Sprints 1–4)

Most of the mainstream surface is at **[Parity]** after the library sprints:

- Architecture (`Models`, providers, auth, stream APIs) — **[Parity]**
- Model catalogs (GPT-5.6, tiers, `max` maps) — **[Parity]** (Hyper is Elph-only)
- Thinking levels including `max` — **[Parity]**
- Deferred / dynamic tools — **[Parity]**
- Cost accounting tiers — **[Parity]**
- Bedrock `apiKey` bearer — **[Parity]**
- Empty thinking + signature (#6457) — **[Parity]**
- Context estimate + compaction boundary (#6464) — **[Parity]**
- Diagnostics + session resource cleanup — **[Parity]**
- Hyper provider — **[Elph delta]** (missing in pi)

---

## Timeline

### 2026-07-11T11:23:28Z @ `4c18610` (v0.80.6 + Unreleased)

**Sprints 1–4 implemented.** Catalogs regenerated from pi; Hyper re-added.

### 2026-07-11T11:12:19Z @ `4c18610` (v0.80.6 + Unreleased)

Initial gap audit.

---

## What landed

### Sprint 1 — foundation

- `ThinkingLevel::Max` — `src/types/mod.rs`, clamp/maps, Anthropic/Bedrock/Google
- `ModelCost.tiers` / `ModelCostTier` — `src/types/mod.rs`
- Tier-aware `calculate_cost` — `src/models/mod.rs`
- Catalog regen + RawCost tiers — `models/*.json`, `src/models/catalog.rs`, `bin/generate_models`

### Sprint 2 — deferred tools

- `Message::ToolResult.added_tool_names` — `src/types/mod.rs`
- `split_deferred_tools` — `src/utils/deferred_tools.rs`
- Anthropic `tool_reference` + `defer_loading` — `src/api/anthropic_messages.rs`
- OpenAI Responses / Codex / Azure tool search — `openai_responses*.rs`, `openai_codex_responses.rs`
- Compat flags — `supports_tool_search`, `supports_tool_references`

### Sprint 3 — correctness

- Empty thinking + valid signature — `anthropic_messages.rs`
- Bedrock bearer from `api_key` — `bedrock_converse_stream.rs`
- Timestamp-aware estimate + added tools — `src/utils/estimate.rs`

### Sprint 4 — polish

- `AssistantMessageDiagnostic` + helpers — `types`, `utils/diagnostics.rs`
- Session resource cleanup registry — `src/session_resources.rs`

---

## Remaining / watch

- After every `generate-models chat`, re-add **Hyper** (`define_catalog!(HYPER_MODELS, …)` + `index.json`) — not in pi.
- **[Gap P1]** Cloudflare key-only credential should fall back to ambient `CLOUDFLARE_ACCOUNT_ID` (pi #6292 / origin after `4c18610`) — check `providers/cloudflare_auth.rs`.
- **[Gap P2]** Retry patterns: gRPC `ResourceExhausted`, Bun `socket connection was closed` — extend `utils/retry.rs` when porting next Unreleased fixes.
- OpenRouter context windows from top provider (#6481) — re-run catalog regen from latest pi.
- OpenAI Completions does not use native deferred tool search (same as pi).

## Elph-only

- Hyper provider + OAuth (`providers/`, `models/hyper.json`, `auth/oauth/hyper.rs`)
