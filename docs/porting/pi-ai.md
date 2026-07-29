# Porting status: pi-ai → elph-ai

**Last audited:** 2026-07-29T20:00:00Z
**Upstream:** `@earendil-works/pi-ai` · `packages/ai` · **v0.82.1** + Unreleased
**Upstream commit:** `cee5ff75`
**Elph crate:** `crates/elph-ai`

---

## At a glance (post Sprint 5)

Most of the pi-ai surface through v0.82.1 is at **[Parity]** after Sprint 5:

- Architecture (`Models`, providers, auth, stream APIs) — **[Parity]**
- Model catalogs (GPT-5.6, tiers, `max` maps) — **[Parity]** (Hyper is Elph-only)
- Thinking levels including `max` — **[Parity]**
- Deferred / dynamic tools — **[Parity]**
- Cost accounting tiers — **[Parity]**
- Bedrock `apiKey` bearer — **[Parity]**
- Empty thinking + signature (#6457) — **[Parity]**
- Context estimate + compaction boundary (#6464) — **[Parity]**
- Diagnostics + session resource cleanup — **[Parity]**
- `contentText` utility — **[Parity]**
- `CredentialStore.list()` — **[Parity]**
- `ModelsStore` + `ModelsStoreEntry.etag` — **[Parity]**
- Retry patterns (DNS, gRPC, socket-drop, HTTP/2, abort-honoring) — **[Parity]**
- `Tool.constrainedSampling` + compat flags — **[Parity]**
- `SessionAffinityFormat` replacing `sendSessionIdHeader` — **[Parity]**
- `ANTHROPIC_AUTH_TOKEN` bearer auth — **[Parity]**
- Auth error messages with cause chain — **[Parity]**
- Hyper provider — **[Elph delta]** (missing in pi)

---

## Timeline

### 2026-07-29 @ `cee5ff75` (v0.82.1 + Unreleased)

**Sprint 5: pi-ai gap port — 7 feature areas.**

Covering changelog entries from v0.80.7 through v0.82.1 (14 releases). See [README.md](./README.md#timeline) for the full list.

- `content_text` / `assistant_content_text` — `src/utils/text.rs`
- `CredentialStore::list()` — `src/auth/types.rs`, `src/auth/credential_store.rs`
- `ModelsStore` trait + `InMemoryModelsStore` + `ProviderStore` — `src/auth/models_store.rs`
- `Usage` metadata on `Message::ToolResult` + `AgentToolResult` — `src/types/mod.rs`, `crates/elph-agent/src/tools/types.rs`
- `Tool.constrained_sampling`, `ConstrainedSamplingConfig`, `StrictMode`, `GrammarVariants` — `src/types/mod.rs`
- `supports_openai_grammar_tools`, `supports_strict_tools` compat flags — `src/types/mod.rs`
- `SessionAffinityFormat` enum — `src/types/mod.rs`
- `ANTHROPIC_AUTH_TOKEN` bearer header — `src/api/anthropic_messages.rs`
- `ModelsError` display includes cause — `src/auth/resolve.rs`
- Enhanced retry patterns (DNS, gRPC, socket-drop, HTTP/2, abort, transient) — `src/utils/retry.rs`

### 2026-07-29 @ `4c18610` (v0.80.6 + Unreleased)

**Test fix: no direct `openai` provider in catalog.**

Two `elph-agent` integration tests used `get_model("openai", "gpt-4o-mini")` which no longer resolves — the model catalog restructured so that `openai` is no longer a directly-registered provider. OpenAI models are now exposed through gateway providers (`kilo`, `sumopod`, `cloudflare-ai-gateway`, `azure-openai-responses`). Tests updated to pick the first available model via `get_models(None).next()`.

No library-level functionality changed — this is a catalog reshape that happened between Sprints 1–4 and now. The `openai.json` model file still exists but the provider registration path changed. If `generate-models chat` is re-run, verify OpenAI registration logic.

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

### Sprint 5 — pi-ai gap port (v0.80.7–v0.82.1)

- **Usage metadata** — `Message::ToolResult.usage` + `AgentToolResult.usage`; propagation from `runtime/exec/messages.rs`
- **ModelsStore** — `src/auth/models_store.rs`: `ModelsStore` trait, `InMemoryModelsStore`, `ProviderStore`, `ModelsStoreEntry.etag`
- **constrainedSampling** — `types/mod.rs`: `ConstrainedSamplingConfig`, `StrictMode`, `GrammarVariants`, `Tool.constrained_sampling`, `Tool::new()` constructor
- **Compat flags** — `supports_openai_grammar_tools` (OpenAI Completions/Responses), `supports_strict_tools` (Anthropic), `supports_strict_mode` (Responses), `SessionAffinityFormat`
- **Retry patterns** — `utils/retry.rs`: +40 patterns (DNS `getaddrinfo`/`ENOTFOUND`/`EAI_AGAIN`, gRPC `ResourceExhausted`, Bun socket-drop, HTTP/2 `goaway`, `previous_response_not_found`), `is_transient_error()` helper
- **Auth correctness** — `api/anthropic_messages.rs`: `ANTHROPIC_AUTH_TOKEN` bearer header from env; `auth/resolve.rs`: `ModelsError` display includes cause chain
- **`contentText`** — `utils/text.rs`: `content_text()`, `assistant_content_text()`
- **`CredentialStore.list()`** — `auth/types.rs` + `credential_store.rs`: `CredentialInfo` + async `list()` method

---

## Remaining / watch

- After every `generate-models chat`, re-add **Hyper** (`define_catalog!(HYPER_MODELS, …)` + `index.json`) — not in pi.
- **[Catalog]** The `openai` provider is no longer directly registered in the catalog. OpenAI models are served through gateway providers (`kilo`, `sumopod`, etc.). Verify `generate-models` still produces correct provider routing when re-run.
- OpenRouter context windows from top provider (#6481) — re-run catalog regen from latest pi.
- OpenAI Completions does not use native deferred tool search (same as pi).
- **[Catalog needed]** Claude Opus 5 model metadata for Anthropic & Bedrock (pi v0.82.1).
- **[P2]** New OAuth providers: Kimi Code subscription, OpenRouter PKCE, Radius pi-messages gateway — implement when provider integration is needed.
- **[P2]** `cacheRetention: "none"` support for disabling implicit prompt-cache writes.
- **[P2]** `retryAssistantCall()` bounded retry lifecycle for transient assistant failures.
- **[P2]** DNS lookup retry (`getaddrinfo`, `ENOTFOUND`, `EAI_AGAIN`) — already added to `is_retryable()`; verify propagation through resilience layer.
- **[P2]** `uuidv7` utility — elph uses `ulid`; pi moved to `uuidv7`. Align if cross-compat needed.
- **[P2]** `toolChoice` for OpenAI/Codex Responses (required + named tool selection) — types exist, provider adapters need wiring.

## Elph-only

- Hyper provider + OAuth (`providers/`, `models/hyper.json`, `auth/oauth/hyper.rs`)
