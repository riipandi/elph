# Porting status: pi-ai → elph-ai

**Last audited:** 2026-08-07T14:00:00Z
**Upstream:** `@earendil-works/pi-ai` · `packages/ai` · **v0.84.1** + Unreleased
**Upstream commit:** `7aca0d7b3`
**Elph crate:** `crates/elph-ai`

---

## At a glance (post Sprint 6)

Most of the pi-ai surface through v0.82.1 + Unreleased is at **[Parity]** after Sprint 6:

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
- `retryAssistantCall()` bounded retry lifecycle — **[Parity]**
- `pending_stop_reason` mid-stream exposure — **[Parity]**
- Per-request `fetch` injection — **[Parity]**
- GitHub Copilot Opus 5 with `minimal` thinking — **[Parity]**
- `samplingParams` passthrough (`StreamOptions` + `OpenAICompletionsCompat`) — **[Parity]** (opt-in via JSON model `compat`)
- `thinking_token_budget` compat flag (vLLM-style reasoning cap) — **[Parity]** (opt-in via JSON model `compat`)
- `supportsFinishReason` stream stop-reason inference — **[Parity]** (default true; opt-out via JSON model `compat`)
- Hyper provider — **[Elph delta]** (missing in pi)

---

## Timeline

### 2026-08-07 @ `7aca0d7b3` (v0.84.1 + Unreleased)

**Sprint 8: P2 gap port — 3 feature areas (opt-in additive).**

Doctrine: each gap is additive and latent until opted in via hand-maintained JSON model `compat`; no existing architecture changed. The generator only copies `compat` from existing JSON and never generates these fields from models.dev, so behavior is unchanged until a model opts in.

- **`samplingParams` passthrough (P2)** — pi v0.84.0 #7568. `crates/elph-ai/src/types/mod.rs`: `sampling_params: Option<HashMap<String, Value>>` on `StreamOptions` and `OpenAICompletionsCompat`. `crates/elph-ai/src/api/openai_completions.rs` `apply_sampling_params` / `apply_sampling_map`: merges model-level defaults with per-request overrides into the OpenAI-completions body without clobbering explicit options (`temperature`, `max_tokens`). Wired through `crates/elph-ai/src/api/simple_options.rs` `build_base_options`.
- **`thinking_token_budget` (P2)** — pi v0.84.0 #7638. `crates/elph-ai/src/types/mod.rs` + `crates/elph-ai/src/api/openai_compat.rs`: `supports_thinking_token_budget` on `OpenAICompletionsCompat` and `ResolvedOpenAICompletionsCompat` (default `false` in `detect_compat`, mapped through `merge_compat`). `crates/elph-ai/src/api/openai_completions.rs` `apply_thinking_token_budget`: emits `thinking_token_budget` to reserve output tokens for vLLM-style providers where reasoning and answer share `max_tokens`.
- **`supportsFinishReason` inference (P2)** — pi v0.84.0. `crates/elph-ai/src/types/mod.rs` + `crates/elph-ai/src/api/openai_compat.rs`: `supports_finish_reason` (default `true`). `crates/elph-ai/src/api/openai_completions.rs` `infer_stop_reason`: when a provider omits streamed `finish_reason` and compat declares it unsupported, infers `StopReason::ToolUse` (any tool call streamed) or `StopReason::Stop` instead of erroring.

### 2026-07-29 @ `cced6a21` (v0.82.1 + Unreleased)

**Sprint 6: P1/P2 gap port — 8 feature areas.**

Covering changelog entries from v0.81.0 through v0.82.1 + Unreleased.

- `retry_assistant_call()` — `src/utils/retry.rs`: bounded retry with exponential backoff, lifecycle callbacks, abort token
- `pending_stop_reason` on `AssistantMessage` — `src/types/mod.rs`: field set mid-stream in Anthropic SSE handler
- `client` field on `StreamOptions` — `src/types/mod.rs`: per-request custom HTTP client injection
- GitHub Copilot Opus 5 `minimal` thinking — `models/github_copilot.json`: added `"minimal"` thinking level mapping
- Fresh routing session IDs for compaction — `src/session/types.rs`: `CheckpointTail` + cursor-based reads

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

**Historical note (superseded):** tests briefly avoided a direct `openai` provider after a catalog reshape. **Current state:** `openai`, `openai-codex`, and `xai` ship catalog models **and** are registered in `builtin_providers()` so stream/auth work end-to-end. `generate-models chat` verifies catalog ids match `builtin_providers()` and fails if a factory is missing.

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

- **[Catalog SSOT]** Chat catalogs origin = **models.dev** via `generate-models chat` / skill **`update-models`**. Not pi `packages/ai` data scripts. Gateways (Hyper, Kilo, TokenRouter, OpenGateway, Sumopod, …) are preserved by the generator — no manual re-add after regen unless dropped from `provider_sources` / `builtin_providers`.
- **[Catalog]** `openai`, `openai-codex`, and `xai` ship catalog models **and** register in `builtin_providers()`; generator fails if a catalog provider lacks a factory.
- OpenRouter / gateway context windows and pricing — refresh with `/update-models` (live pricing when keys allow), not pi JSON seed.
- OpenAI Completions does not use native deferred tool search (same as pi).
- **[P2]** OAuth providers already implemented for Kimi / OpenRouter / Radius — watch for upstream protocol drift, not re-port from scratch.
- **[P2]** `uuidv7` utility — elph uses `ulid`; pi moved to `uuidv7`. Align if cross-compat needed.
- **[P2]** `toolChoice` for OpenAI/Codex Responses (required + named tool selection) — types exist, provider adapters need wiring.
- **[P2]** Opt-in compat flags are now live but latent (`sampling_params`, `supports_thinking_token_budget`, `supports_finish_reason`). They take effect only when a hand-maintained JSON model opts in via its `compat` block; no generator change needed.
- **[P2]** `rawStopReason` field on stream output — pi v0.84.0 captures the raw `finish_reason` string alongside the mapped `StopReason`. Watch if callers need the raw value.
- **[P2]** Deferred provider request contracts (#7339) — pi v0.84.0 adds `DeferredHandle`, `Provider.fetchDeferred`/`cancelDeferred`, durable response handles, and faux-provider async-response support. Distinct from the deferred-tools feature already at **[Parity]**. Not ported; only needed if a provider requires async/deferred response fetching.
- **[P2]** v4 dynamic provider refresh context — pi v0.84.0 replaces `RefreshModelsContext.store` with read-only `context.stored` + generation-checked `context.publish()` transaction, and guarantees a concrete `signal`. Elph's catalog is mostly models.dev-static, so impact is low; watch if dynamic provider refresh grows.
- **[Partial]** Structured Bedrock failure diagnostics (error code + AWS request id, #7286) — Elph uses `aws_sdk_bedrockruntime`; errors surface through the SDK but modeled error codes / request ids aren't explicitly extracted.

## Elph-only

- Hyper provider + OAuth (`providers/`, `models/hyper.json`, `auth/oauth/hyper.rs`)
- Kilo provider OAuth (device code + org selection, `auth/oauth/kilo.rs`, ported from `Kilo-Org/kilo-pi-provider`)
- Hugging Face OAuth (device code, `auth/oauth/huggingface.rs`, ported from `osolmaz/pi-huggingface-oauth`)
- models.dev catalog pipeline (`bin/generate_models/`: `models_dev`, `provider_sources`, `thinking_map`, …)
- OpenAI-compat gateway hardening + tool schema sanitize for non-standard gateways
