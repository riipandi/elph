# Porting status (upstream → Elph)

How far Elph crates lag (or lead) upstream **pi** projects:

- TypeScript **[earendil-works/pi](https://github.com/earendil-works/pi)** → `elph-ai`, `elph-agent`, `elph/`

**Readability:** these pages prefer short prose, bullets, and timeline entries.
Avoid packing status into wide tables.

## Documents

- **[pi-ai.md](./pi-ai.md)** — `@earendil-works/pi-ai` (`packages/ai`) → `crates/elph-ai`
- **[pi-agent.md](./pi-agent.md)** — `@earendil-works/pi-agent-core` (`packages/agent`) → `crates/elph-agent`
- **[pi-coding-agent.md](./pi-coding-agent.md)** — `@earendil-works/pi-coding-agent` (`packages/coding-agent`) → `elph/` (product CLI + TUI)
- **[feature-comparison.md](./feature-comparison.md)** — Detailed feature-by-feature table across all four crates

## Why these docs exist

Upstream projects move quickly. Each page records:

1. What upstream has.
2. What the port has (Elph).
3. Gaps in either direction — port debt vs intentional product extensions.

## Baseline (pi libraries)

Last documented **2026-07-29T19:50:00Z**.

- **Upstream:** https://github.com/earendil-works/pi
- **Local clone (analysis):** `/Users/ariss/Developer/github.com/earendil-works/pi`
- **Snapshot commit:** `cced6a21` (_fix(coding-agent): stop loading AGENTS.md twice in nested git worktrees_)
- **Package version:** `0.82.1` (released 2026-07-25) + **Unreleased** on `main`
- **Mapping:** `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent`, `packages/coding-agent` → `elph/`
- **Last library implementation pass:** 2026-07-29 — Sprint 7: remaining P2 gap port (Kimi OAuth, OpenRouter OAuth, Radius OAuth, pi-messages, JsonlSessionStorage, file mutation queue, image tool)
- **Last product gap audit:** 2026-07-29 — dead code cleanup + clippy hardening across `elph/` TUI modules

## Status tags

Use these inline in prose (not table cells):

- **[Parity]** — behavior/API on both sides (shape may differ by language)
- **[Partial]** — present in the port but incomplete vs mainstream
- **[Gap]** — in upstream; not yet in the port (port debt)
- **[Elph delta]** — intentional extension missing upstream
- **[N/A]** — platform-specific; do not port 1:1

## Suggested sync workflow

### Pi → elph crates

1. Update the local pi clone: `git pull` in the clone path.
2. Read upstream changelogs (`packages/ai/CHANGELOG.md`, `packages/agent/CHANGELOG.md`).
3. Diff against the timeline / remaining sections in this folder (prose, not tables).
4. Port + regenerate catalogs when needed:

    ```sh
    # Catalog path is fixed: ../../earendil-works/pi/packages/ai (from elph workspace root)
    cargo run -p elph-ai --bin generate-models -- chat --skip-scripts
    # Then re-add Elph-only providers (Hyper, OpenGateway, Kilo, …) if wiped.
    ```

5. Append a **Timeline** entry with ISO timestamp + pi commit/version (bullet prose).

### Timeline

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

**Scope:** `elph/` product crate + `elph-tui`, `elph-agent` tests.

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
