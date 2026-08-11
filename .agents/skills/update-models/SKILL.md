---
name: update-models
description: >-
    Refresh elph-ai chat model catalogs from models.dev (origin) with optional live
    provider pricing, resolve thinkingLevelMap from actual sources (provider API ->
    models.dev fallback), and verify provider registration. Use when the user runs
    /update-models, asks to sync or regenerate model catalogs, update pricing, or
    keep provider model lists current.
metadata:
    scope: project
---

# Update Models (models.dev origin)

## Language

- In-chat reports follow the user's language.
- Docs/skill text and generated comments stay English.

## Purpose

Regenerate `crates/elph-ai/models/*.json` (+ `models/index.json`) from **models.dev** as the sole catalog origin.
Pricing prefers live provider APIs when keys/endpoints allow, then models.dev. Every model includes a complete
`thinkingLevelMap`, resolved from real sources — never invented or copy-pasted across models.
Each model also carries a `thinkingLevelMapSource` field tracking where the map came from.

The JSON files are the only catalog source: `crates/elph-ai/build.rs` compresses them into the binary on the next build, so there is no Rust catalog file to regenerate.

## When to run

- `/update-models`
- "refresh model catalog", "sync models.dev", "regenerate models", "update pricing"

## Prerequisites

- Network access (unless `--offline` with existing `models/.cache/models.dev/api.json`)
- Optional env keys for live pricing/capability probes (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `HYPER_API_KEY`, `INFRON_API_KEY`, …)
- No local pi clone required for chat catalogs

## Workflow

1. **Generate chat catalogs**

```sh
cargo run -p elph-ai --bin generate-models -- chat && make fmt
# or
make generate-models ARGS="chat" && make fmt
```

Useful flags:

| Flag                | Effect                                                        |
| ------------------- | ------------------------------------------------------------- |
| `--offline`         | Use cached models.dev only                                    |
| `--no-live-pricing` | Skip provider `/models` pricing/capability probes             |
| `--force`           | Bypass the models.dev cache freshness check (always re-fetch) |

Each model's `thinkingLevelMap` is resolved per the precedence in **Thinking level sourcing** below — this happens automatically inside `thinking_map.rs`, not as a separate manual step.

2. **Optional full pass** (chat + image fixture path)

```sh
cargo run -p elph-ai --bin generate-models -- all --no-live-pricing
```

3. **Rebuild the binary**

`build.rs` compresses `models/*.json` into the binary at compile time, so the new catalogs only ship after a rebuild. `cargo check`/`cargo test` (next step) trigger this automatically, but build the real target to bake in the compressed catalogs:

```sh
cargo build --release -p elph        # ships compressed catalogs into the single binary
```

4. **Verify**

```sh
cargo test -p elph-ai --test providers --lib models
cargo check -p elph -p elph-ai
```

Confirm:

- `thinkingLevelMap: complete=N incomplete=0` in generator output
- `thinkingLevelMap source breakdown: live-api=X models-dev=Y provider-override=Z previous=W unresolved=V` — `unresolved` must be 0 for reasoning=true models, or explicitly reported to the user (never silently filled)
- `Verified … catalog providers are registered in builtin_providers()`
- Spot-check `anthropic.json`, `xai.json`, a gateway (`openrouter` / `kilo` / `hyper` / `infron`) — for at least one model per file, confirm the `thinkingLevelMap` values match what the source (provider API docs/response or models.dev entry) actually reports, not a guess.

5. **Summarize for the user**

- Provider count / model count
- `thinkingLevelMap` source breakdown (live-api vs models.dev vs provider-override vs previous vs unresolved)
- Any models where thinking support could not be confirmed from any source (report explicitly — do not silently mark as `off` or duplicate a sibling model's map)
- Any providers skipped (not on models.dev and no previous overlay)
- Remaining zero-priced models if any
- Reminder: do not hand-edit generated catalogs except intentional Elph overlays; re-run this skill after changes

## Thinking level sourcing

`thinkingLevelMap` must reflect what each model **actually** supports, not a static/hardcoded table applied
across a provider or model family. Resolve per model, in this order:

1. **Live provider API (primary)** — if the provider's `/models` endpoint (or a dedicated capabilities
   endpoint) exposes `reasoning.supported_efforts` for that exact model id, use it directly. Map the
   provider's native levels (e.g. `low/medium/high`, `minimal/low/medium/high`, boolean `reasoning: true`) onto
   the 7-key schema; do not invent intermediate levels the provider doesn't expose — mark them `null`.
   Supported effort aliases: `"none"` → `"off"`, `"min"` → `"minimal"`, `"default"` → `"medium"`.

2. **models.dev (secondary)** — if the live API doesn't expose per-model thinking capability (most providers
   don't), look up the **same model id** in the cached models.dev entry and use its `reasoning_options`
   metadata if present. Extract `effort`-type options and map their `values` array onto the 7-key schema.

3. **Provider-family override (tertiary)** — when neither live API nor models.dev has data, fall back to
   known provider defaults from official documentation:
   - **xAI Grok**: low / high / max
   - **Anthropic Opus/Sonnet-5/Fable**: xhigh / max (adaptive thinking)
   - **Anthropic Haiku 4.5**: low / medium / high / max
   - **Anthropic Sonnet 4.5 / Opus 4.5 / earlier 4.x**: low / medium / high / max
   - **OpenAI GPT-5.x reasoning models**: off / low / medium / high / xhigh
   - **OpenAI O-series**: low / medium / high
   These overrides also handle gateway-prefixed model IDs (e.g. `openai/gpt-5.4` on OpenRouter) by
   extracting the base model id after the last slash.

4. **Elph overlays (preserved)** — if the previous catalog had a thinkingLevelMap with at least one
   non-null wire value, preserve it. This protects intentional hand-authored overrides from being
   overwritten by stale defaults.

5. **Unresolved (no silent fill)** — if no source has capability data for that model id:
    - Do **not** copy the map from a "similar" model (different id) in the same family.
    - Do **not** default to all-`null` or all-`off` as if that were confirmed.
    - Emit it as `unresolved` in the generator output and surface it in the final summary so a human decides
      (e.g. via an Elph overlay override), instead of the generator quietly guessing.

Every resolved entry should be traceable back to (a) a live API response field, (b) a specific models.dev
field, (c) an explicit provider override from official docs, or (d) an explicit Elph overlay — never to
inference from the model name or family.

## Data freshness

The generator keeps model data current through three layers:

1. **models.dev cache** — `models/.cache/models.dev/api.json` is reused when younger than **24h**. Use `--force` to bypass. On a fetch failure (network or non-2xx), the cached snapshot is used as a fallback instead of failing.

2. **Live pricing & capability probes** — for providers with `live_pricing_base` + `live_pricing_env` set (and the env key present), `/models` is probed for per-model pricing and, where exposed, thinking/reasoning capability. Supported pricing shapes:
    - models.dev style: `metadata.pricing.{input_per_million, output_per_million, cached_input_per_million}`
    - Hyper style: `pricing.{input, output, cache_hit, cache_create}`
    - Infron/OneRouter style: `min_prompt_price` / `min_completion_price`
    
    Thinking capabilities are extracted from `reasoning.supported_efforts` (array of effort strings)
    and `reasoning.mandatory` / `reasoning.default_enabled` booleans.

3. **Live model list (gateway providers)** — for `gateway_preserve_ids` providers with a live `/models` endpoint, the **live id list replaces the previous catalog ids** (source of truth). New upstream models appear automatically; removed ones drop out. When the API exposes `category_type`, only `LLM` entries are kept so image/video models never pollute the chat catalog. If no live endpoint/key is available, the previous catalog ids are preserved.

## Rules

- **Origin**: models.dev only — never seed chat catalogs from pi.
- **Pricing**: live API (when available) → models.dev → keep previous non-zero.
- **Every model** must have `thinkingLevelMap` with keys `off|minimal|low|medium|high|xhigh|max` (null = unsupported), resolved per **Thinking level sourcing** — live API → models.dev → provider override → preserved overlay → explicit unresolved report. Never invented, never copied from a sibling model.
- **Do not** invent `api` / `baseUrl` from models.dev; preserve Elph factory overlays.
- **Do not** remove Elph-only gateway providers; preserve their model ids and enrich.
- New providers still need a factory in `src/providers/builtin.rs` (generator fails if unregistered).
- Schema contract for user overrides: `schemas/provider-schema.json`.
- **Never** hand-write a Rust catalog module — `models/*.json` is the source, `build.rs` embeds it.

## Code map

| Path                                      | Role                                                                                                        |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `bin/generate_models/main.rs`             | CLI                                                                                                         |
| `bin/generate_models/models_dev.rs`       | Fetch/cache models.dev (24h TTL + fallback)                                                                 |
| `bin/generate_models/provider_sources.rs` | Elph ↔ models.dev map + live endpoint config                                                                |
| `bin/generate_models/normalize.rs`        | Entry merge + cost fields + persist `thinkingLevelMapSource`                                                 |
| `bin/generate_models/thinking_map.rs`     | Resolves thinkingLevelMap per model: live-api → models.dev → provider-override → preserved overlay → unresolved (source-tagged, no invented values) |
| `bin/generate_models/pricing.rs`          | Live pricing probes + live model id refresh + thinking capability extraction                                  |
| `bin/generate_models/chat.rs`             | Orchestration + registration check + source breakdown summary                                                 |
| `models/*.json`                           | Catalog source (compressed into the binary)                                                                 |
| `build.rs`                                | zstd frames + provider index for the binary                                                                 |
| `src/models/catalog.rs`                   | Lazy loader (seed + CONFIG_DIR overlay)                                                                     |
| `schemas/provider-schema.json`            | User override schema (merged by install_provider_catalog_dir)                                               |

## Do not

- Commit without the user asking
- Run live pricing/capability probes against unpaid keys in a loop
- Hand-edit hundreds of models when a generator fix exists
- Fill `thinkingLevelMap` by guessing from model name/family when no source (live API, models.dev, or Elph overlay) confirms it
- Preserve incorrect previous maps when fresh source data is available — fresh data always takes priority over stale catalog entries
