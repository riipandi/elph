# Porting status (upstream → Elph)

How far Elph crates lag (or lead) upstream **pi** projects:

- TypeScript **[earendil-works/pi](https://github.com/earendil-works/pi)** → `elph-ai`, `elph-agent`, `elph/`

**Readability:** these pages prefer short prose, bullets, and timeline entries.
Avoid packing status into wide tables.

## Documents

- **[pi-ai.md](./pi-ai.md)** — `@earendil-works/pi-ai` (`packages/ai`) → `crates/elph-ai`
- **[pi-agent.md](./pi-agent.md)** — `@earendil-works/pi-agent-core` (`packages/agent`) → `crates/elph-agent`
- **[pi-coding-agent.md](./pi-coding-agent.md)** — `@earendil-works/pi-coding-agent` (`packages/coding-agent`) → `elph/` (product CLI + TUI)

## Why these docs exist

Upstream projects move quickly. Each page records:

1. What upstream has.
2. What the port has (Elph).
3. Gaps in either direction — port debt vs intentional product extensions.

## Baseline (pi libraries)

Last documented **2026-07-29T20:00:00Z**.

- **Upstream:** https://github.com/earendil-works/pi
- **Local clone (analysis):** `/Users/ariss/Developer/github.com/earendil-works/pi`
- **Snapshot commit:** `cee5ff75` (_ref: remove openclaw reference from readme_)
- **Package version:** `0.82.1` (released 2026-07-25) + **Unreleased** on `main`
- **Mapping:** `packages/ai` → `elph-ai`, `packages/agent` → `elph-agent`, `packages/coding-agent` → `elph/`
- **Last library implementation pass:** 2026-07-29 — Sprint 5: pi-ai gap port (usage metadata, ModelsStore, constrainedSampling, retry patterns, auth correctness, contentText, CredentialStore.list)
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
