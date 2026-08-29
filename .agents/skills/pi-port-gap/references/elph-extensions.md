# Elph product-delta scan hints

Starting checklist for **Phase 3 (Elph implementation delta)**. Always verify in
code; the list grows over time. These are **not** port gaps.

For each item found, write **In pi / In Elph / Implications** — not a one-line badge.

When a future pi feature **converges** with an item here, reclassify toward
`[Partial]` / `[Parity]` under **Parity and nuance**, and implement on **Elph
architecture** (see SKILL.md **Porting doctrine** + **Architecture invariants**).

---

## elph-ai — catalog & providers (often no 1:1 pi path)

### Model catalogs (settled divergence)

- **Origin = models.dev**, not pi `packages/ai` data scripts / npm `generate-models`.
- Generator: `crates/elph-ai/bin/generate_models/`
    - `models_dev.rs` — fetch/cache `https://models.dev/api.json`
    - `provider_sources.rs` — Elph provider id ↔ models.dev keys, defaults, gateway flags
    - `normalize.rs` / `thinking_map.rs` / `pricing.rs` / `chat.rs`
- Output: `models/*.json`, `models/index.json` (no generated Rust catalog — `build.rs` compresses the JSON into the binary and `src/models/catalog.rs` loads it lazily, merged over `CONFIG_DIR/providers/*.json`)
- Skill: **`update-models`** (`.agents/skills/update-models/SKILL.md`)
- **Every model** has full `thinkingLevelMap` (keys: `off|minimal|low|medium|high|xhigh|max`)
- Registration gate: catalog provider ids ⊆ `builtin_providers()` (generator fails if missing)
- Forward-looking override schema: `schemas/provider-schema.json` (runtime merge later)

**Implications for porting:** pi CHANGELOG “new models / regenerate catalog” → run Elph
`generate-models chat` / `/update-models`, adjust `provider_sources` or overlays — do **not**
copy pi JSON or reintroduce `--catalog-dir` / pi npm chat generation.

### Elph-only or heavily customized providers

- **Hyper** — `models/hyper.json`, OAuth + completions
- **Kilo / TokenRouter / OpenGateway / Sumopod / Baseten / Neuralwatt / Ollama Cloud** — gateway-style catalogs; preserve route ids, enrich from models.dev
- **Faux provider** — deterministic tests (`faux_*`)
- **OpenAI-compat gateway hardening** — `src/api/openai_compat.rs` non-standard defaults; tool schema sanitize (`src/utils/tool_schema.rs`) for xAI/etc.
- **Thinking wire maps** — `map_thinking_level_for_api`, clamp/cycle for product TUI (elph crate)

### Other elph-ai

- **Session resource cleanup** — `src/session_resources.rs` (confirm vs pi if later shared)
- **Resilience** — circuit breaker / rate limit stack under `src/resilience/`
- **OAuth set** — Anthropic, GitHub Copilot, OpenAI Codex, OpenRouter, Hyper, Kimi, xAI, … (`src/auth/oauth/`)

## elph-agent (product / runtime additions)

- **MCP client** — `src/tools/mcp/`
    - config merge (home + project), schema validate
    - transports: stdio, streamable HTTP, SSE
    - auth: env/token vs OAuth store, conflict policy
    - crypto: AES-256-GCM per-field `enc:`, `auth.json` + wrapped key at `auth.lock` (machine-bound)
    - registry, session pool, policy, truncate, events/progress
    - tool names: `mcp_{server}__{tool}`
- **Goals** — `src/goals/`
- **Subagent** — `src/subagent/`
- **Built-in tools** — `src/tools/` (read, shell_exec, grep, web, …)
- **Mode / plan** — `src/mode/`
- **Sandbox** — `src/sandbox/`
- **Datastore / Turso** — `src/datastore/`, session Turso backends
- **Prompt encoding (TOON)** — `src/runtime/` / prompt encoding env
- **Harness extras** — richer than pi-agent-core (lifecycle hooks, compaction wiring)
- **Skills / prompt templates** — `src/skills/`, `src/prompt_templates/`

## elph product (only if scope expands to coding-agent)

- Model picker: configured-only filter (`settings.models.showConfiguredOnly`), search grouped by provider
- Thinking Ctrl+. / footer: catalog-driven cycle + clamp on model switch
- Provider connect CLI/TUI + auth.json credential store

## How to confirm “missing in pi”

```sh
# From pi clone — make sure it's tracking live main, not a stale checkout
git fetch origin main && git checkout main && git pull && git log -1 --oneline

ls packages/agent/src packages/ai/src
rg -n "mcp|MCP" packages/agent packages/ai --glob '!**/node_modules/**' | head
# From elph
ls crates/elph-agent/src
rg -n "pub mod" crates/elph-agent/src/lib.rs
# Catalog path (Elph, not pi)
ls crates/elph-ai/bin/generate_models
head -20 crates/elph-ai/bin/generate_models/main.rs
```

If pi later adds a similar concept (e.g. native MCP, or a different catalog host), reclassify from
`[Elph delta]` toward `[Partial]` / convergence notes under **Parity and nuance** — and still
implement on Elph’s modules, not by importing pi’s generator.
