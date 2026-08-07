# Porting status (upstream → Elph)

How far Elph crates lag (or lead) upstream **pi** projects:

- TypeScript **[earendil-works/pi](https://github.com/earendil-works/pi)** → `elph-ai`, `elph-agent`, `coding-agent`

**Readability:** these pages prefer short prose, bullets, and timeline entries.
Avoid packing status into wide tables.

## Documents

- **[pi-ai.md](./pi-ai.md)** — `@earendil-works/pi-ai` (`packages/ai`) → `crates/elph-ai`
- **[pi-agent.md](./pi-agent.md)** — `@earendil-works/pi-agent-core` (`packages/agent`) → `crates/elph-agent`
- **[pi-coding-agent.md](./pi-coding-agent.md)** — `@earendil-works/pi-coding-agent` (`packages/coding-agent`) → `crates/coding-agent/` (product CLI + TUI)
- **[feature-comparison.md](./feature-comparison.md)** — Detailed feature-by-feature table across all four crates

## Why these docs exist

Upstream projects move quickly. Each page records:

1. What upstream has.
2. What the port has (Elph).
3. Gaps in either direction — port debt vs intentional product extensions.

## Baseline (pi libraries)

Last documented **2026-07-29T19:50:00Z** (pi snapshot). Catalog SSOT note updated **2026-08-01**.

- **Upstream:** https://github.com/earendil-works/pi
- **Local clone (analysis):** `/Users/ariss/Developer/github.com/earendil-works/pi`
- **Snapshot commit:** `7aca0d7b3` (_fix(agent): make JSONL decode errors explicit_)
- **Package version:** `0.84.1` (released 2026-08-07) + **Unreleased** on `main`
- **Mapping:** `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent`, `packages/coding-agent` → `crates/coding-agent/`
- **Last gap audit:** 2026-08-07 — full walk from `cced6a21` (v0.82.1) to `7aca0d7b3` (v0.84.1), 404 commits; the dominant change is a v4 lane-based session/repo overhaul in pi-agent (`packages/agent/src/harness/session/`).
- **Last library implementation pass:** 2026-08-07 — Sprint 8: safe additive Pi gaps (BeforeToolCallResult.terminate, Agent.reset idle-guard, samplingParams, thinking_token_budget, supportsFinishReason).
- **Catalog SSOT (settled):** chat models from **models.dev** via `elph-ai` `generate-models` / skill `update-models` — **not** from pi `packages/ai` data. Port gap analysis skill: `pi-port-gap` (adopt intent, implement Elph shape).

## Status tags

Use these inline in prose (not table cells):

- **[Parity]** — behavior/API on both sides (shape may differ by language)
- **[Partial]** — present in the port but incomplete vs mainstream
- **[Gap]** — in upstream; not yet in the port (port debt)
- **[Elph delta]** — intentional extension missing upstream
- **[N/A]** — platform-specific; do not port 1:1

## Suggested sync workflow

### Pi → elph crates

**Doctrine:** adopt **gap intent** from pi; implement on **Elph architecture**.
Do not copy pi’s TypeScript layout or catalog scripts. Agent skill:
[`.agents/skills/pi-port-gap`](../../.agents/skills/pi-port-gap/SKILL.md).

1. Update the local pi clone: `git pull` on `main` in the clone path.
2. Read upstream changelogs (`packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`) and, if needed, `git diff` of `packages/ai/src` / `packages/agent/src` vs last audit (CHANGELOG lags).
3. Diff against the timeline / remaining sections in this folder (prose, not tables). Classify each item as **runtime gap** vs **catalog data** vs **already covered by Elph**.
4. **Catalog / model lists** are **not** seeded from pi. Origin is [models.dev](https://models.dev):

    ```sh
    # Elph catalog SSOT — skill: .agents/skills/update-models
    cargo run -p elph-ai --bin generate-models -- chat
    # offline after a prior fetch:
    cargo run -p elph-ai --bin generate-models -- chat --offline --no-live-pricing
    cargo test -p elph-ai --test providers catalog_providers_match_builtin_providers
    ```

    Gateways (Hyper, Kilo, TokenRouter, OpenGateway, Sumopod, …) are **preserved** by the generator; do not use obsolete `--catalog-dir` / pi npm `generate-models` for chat.

5. **Runtime gaps** (API adapter, auth, stream flag, tool schema, agent loop) → implement in `crates/elph-ai` / `crates/elph-agent` following existing modules — not by importing pi packages.
6. Append a **Timeline** entry with ISO timestamp + pi commit/version (bullet prose).

### Timeline

### 2026-08-07 — Sprint 8: safe additive Pi gap port (5 features)

**Scope:** `elph-ai` + `elph-agent` library crates. Upstream commit `7aca0d7b3` (v0.84.1 + Unreleased). Doctrine applied: no existing Elph architecture was changed; each gap was ported as an additive, opt-in complement on top of the current shape.

- **`BeforeToolCallResult.terminate` (P2)** — pi v0.84.1 #7715. `crates/elph-agent/src/runtime/loop_config.rs`: new `terminate: Option<bool>` field. `crates/elph-agent/src/runtime/exec/prepare.rs`: a blocked `beforeToolCall` result now propagates `terminate` into the `AgentToolResult.terminate`, feeding the existing `should_terminate_tool_batch` batch-early-termination rule. **[Parity]** for the before-hook path (the after-hook path was already ported).
- **`Agent.reset()` idle-guard (P2)** — pi v0.84.1 #7717. `crates/elph-agent/src/agent/mod.rs`: `reset()` now returns `Result<(), anyhow::Error>` and bails with `"Agent is already processing. Wait for completion before resetting."` when an `activeRun` is in flight, instead of clearing transcript/runtime state mid-run. No external callers; faithful Rust port of the pi throw.
- **`samplingParams` passthrough (P2)** — pi v0.84.0 #7568. `crates/elph-ai/src/types/mod.rs`: new `sampling_params: Option<HashMap<String, Value>>` on `StreamOptions` and `OpenAICompletionsCompat`. `crates/elph-ai/src/api/openai_completions.rs` `apply_sampling_params` / `apply_sampling_map`: merges model-level defaults with per-request overrides into the OpenAI-completions body without clobbering explicit options (`temperature`, `max_tokens`). Latent until opted in via JSON model `compat`.
- **`thinking_token_budget` compat flag (P2)** — pi v0.84.0 #7638. `crates/elph-ai/src/types/mod.rs` + `crates/elph-ai/src/api/openai_compat.rs`: new `supports_thinking_token_budget` on `OpenAICompletionsCompat` / `ResolvedOpenAICompletionsCompat` (default `false` in `detect_compat`, mapped through `merge_compat`). `crates/elph-ai/src/api/openai_completions.rs` `apply_thinking_token_budget`: when opted in, emits `thinking_token_budget` to reserve output tokens for vLLM-style providers where reasoning and answer share `max_tokens`.
- **`supportsFinishReason` inference (P2)** — pi v0.84.0. `crates/elph-ai/src/types/mod.rs` + `crates/elph-ai/src/api/openai_compat.rs`: new `supports_finish_reason` (default `true`). `crates/elph-ai/src/api/openai_completions.rs` `infer_stop_reason`: when a provider omits streamed `finish_reason` and compat declares it unsupported, the adapter infers `StopReason::ToolUse` (if any tool call was streamed) or `StopReason::Stop` instead of erroring the stream.

**Gap — not ported (intentional):** the pi-agent v4 lane-based session/repo model (`packages/agent/src/harness/session/{types,session,state,memory,jsonl/search}.ts`, `reducer.ts`, `telemetry.ts`). This is a ground-up architectural rewrite (lanes, durable operation records, `SessionRepo` contract, `findOpenOperations` recovery, `FileSystem.renameFile` requirement) that would reshape Elph's existing tree-entry `SessionStorage` design. Remains **[Gap P1 architectural]** — deferred pending an explicit architecture decision, not a safe additive port. See [pi-agent.md](./pi-agent.md#remaining--watch).

Details in [pi-ai.md](./pi-ai.md#timeline) and [pi-agent.md](./pi-agent.md#timeline).

### 2026-08-01 — Catalog SSOT cutover + porting doctrine

**Scope:** `elph-ai` generator + agent skills / porting docs (not a pi version bump).

- Chat catalogs: origin **models.dev** (`generate-models chat`); full `thinkingLevelMap` on every model; live pricing preferred when available
- Gateways preserved (Hyper, Kilo, TokenRouter, OpenGateway, Sumopod, …); registration gate vs `builtin_providers()`
- Skills: **`update-models`** for catalog regen; **`pi-port-gap`** doctrine = adopt pi _gaps only_, implement on Elph architecture (no pi JSON seed, no dual SSOT)
- Obsolete for chat: `--catalog-dir` / pi npm generate / “re-add Hyper after wipe”

### 2026-07-29 — Sprint 5: pi-ai gap port (7 features)

**Scope:** `elph-ai` + `elph-agent` library crates.

- **Usage metadata** — `Message::ToolResult.usage` + `AgentToolResult.usage` with full propagation from tool execution to transcript
- **ModelsStore** — trait + `InMemoryModelsStore` + `ProviderStore` with `etag` support for conditional catalog refresh
- **constrainedSampling** — `ConstrainedSamplingConfig`, `StrictMode`, `GrammarVariants`, `Tool.constrained_sampling`, compat flags (`supports_openai_grammar_tools`, `supports_strict_tools`, `supports_strict_mode`)
- **Retry patterns enhanced** — +40 patterns: DNS lookup failures, gRPC `ResourceExhausted`, Bun socket-drop, HTTP/2 errors, `is_transient_error()` helper
- **`contentText` utility** — `content_text()` / `assistant_content_text()` extractors
- **`CredentialStore.list()`** — async non-secret credential enumeration
- **Auth correctness** — `ANTHROPIC_AUTH_TOKEN` bearer header for Anthropic-compatible gateways; `ModelsError` display includes cause chain
- **`SessionAffinityFormat`** enum replacing `sendSessionIdHeader` boolean

Details in [pi-ai.md](./pi-ai.md) and [pi-agent.md](./pi-agent.md).

### 2026-07-29 — Sprint 6: P1/P2 gap port (8 feature areas)

**Scope:** `elph-ai` + `elph-agent` library crates. Upstream commit `cced6a21`.

- **`AgentHarnessTool` + `toolContext` (P1)** — pi v0.82.0 pattern: `ToolContext` struct replaces captured `Arc<LocalExecutionEnv>` in tool factories. New `AgentHarnessTool` trait, `context_aware_tool()` helper. `ToolExecuteFn` signature extended with `ToolContext`. Threaded through `AgentLoopConfig`, `execute_prepared_tool_call`, and dispatch. `shell_exec` migrated to context-aware execution.
- **`SessionStorage` API v2 (P1)** — pi v0.81.0 breaking changes: `SessionStatistics`, `CursorPosition`, `CheckpointTail` types. New trait methods: `get_path_to_root_or_compaction()`, `get_entries_cursor()`, `get_statistics()`, `store_checkpoint_tail()`, `load_checkpoint_tail()`, `list_checkpoint_tails()`, `get_name()`. All three backends (InMemory, SessionDir, Turso) implemented.
- **Compaction retry lifecycle (P2)** — pi v0.81.1: `compact_with_retry()` with exponential backoff (1s, 2s, 4s, max 3 retries). `CompactionRetryEvent` enum with `Attempt`/`Retry`/`Recovered`/`Failed` variants. Events emitted via `AgentHarnessOwnEvent`.
- **GitHub Copilot Opus 5 `minimal` thinking (P1)** — Added `"minimal": "minimal"` thinking level mapping to `claude-opus-5` model in `github_copilot.json`.
- **Per-request fetch injection (P2)** — pi Unreleased: `client: Option<reqwest::Client>` field on `StreamOptions`; propagated through `simple_options`.
- **Pending stop reason while streaming (P2)** — pi Unreleased: `pending_stop_reason: Option<StopReason>` on `AssistantMessage`. Set mid-stream in Anthropic SSE handler when `delta/stop_reason` is received.
- **`retryAssistantCall()` (P2)** — pi v0.82.0: bounded retry for transient assistant failures with exponential backoff, lifecycle callbacks, and abort token support. Uses `is_transient_error()` for error classification.
- **Fresh routing session IDs for compaction (P2)** — pi v0.82.0: compaction and branch-summary requests use fresh routing session IDs through the checkpoint tail mechanism.

### 2026-07-29 — Sprint 7: remaining P2 gap port (7 feature areas)

**Scope:** `elph-ai` + `elph-agent` library crates.

- **Kimi Code OAuth (P2)** — `elph-ai/src/auth/oauth/kimi.rs`: device code flow, token refresh, registered in OAuth registry and `kimi_coding_provider()`.
- **OpenRouter OAuth PKCE (P2)** — `elph-ai/src/auth/oauth/openrouter.rs`: PKCE login, API key minting, token refresh, registered in OAuth registry.
- **Radius OAuth gateway (P2)** — `elph-ai/src/auth/oauth/radius.rs`: PKCE login for Inflection AI pi-messages gateway, registered in OAuth registry.
- **pi-messages gateway API (P2)** — `elph-ai/src/api/pi_messages.rs`: `PiMessagesApi` implementing `ProviderStreams`, registered in built-in API registry.
- **JsonlSessionStorage (P2)** — `elph-agent/src/session/backends/jsonl.rs`: JSONL-backed session storage backend, implementing full `SessionStorage` v2 API.
- **File mutation queue (P2)** — `elph-agent/src/tools/file_mutation_queue.rs`: `FileMutationQueue` for serializing file mutations with apply/rollback support.
- **Image tool (P2)** — `elph-agent/src/tools/image.rs`: `create_image_tool()` for reading image metadata, supporting PNG/JPG/GIF/WebP/BMP/SVG formats.

Details in [feature-comparison.md](./feature-comparison.md).

### 2026-07-29 — Rust verify & harden + dead code cleanup

**Scope:** `crates/coding-agent/` product crate + `elph-tui`, `elph-agent` tests.

- `make lint` brought to zero violations: 26 clippy errors fixed across 5 files.
- `make test` repaired: 2 `elph-agent` tests broke due to model catalog restructure (direct `openai` provider removed; models now served through gateway providers). Updated to use `get_models(None).next()`.
- Dead code removed: 17 items across provider connect dialog, credential store, plan confirmation, paths, and tool approval modules.
- All 1881 tests passing, lint clean, warnings-free.

Details in [pi-coding-agent.md](./pi-coding-agent.md#timeline).

## Skills

- **`/pi-port-gap`** — pi libraries/product vs elph crates

## Related

- [`crates/elph-ai/README.md`](../../crates/elph-ai/README.md)
- [`crates/elph-agent/README.md`](../../crates/elph-agent/README.md)
- [docs/README.md](../README.md)
