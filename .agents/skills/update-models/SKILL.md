---
name: update-models
description: >-
    Refresh elph-ai chat model catalogs by compiling multiple authoritative sources
    (models.dev api/models/catalog, OpenRouter live API, Nara /api/pricing, ai-model-directory) with
    optional live provider pricing probes. Pricing and thinkingLevelMap are always
    filled and refreshed; OpenRouter `supported_efforts` drives thinking levels.
    Use when the user runs /update-models, asks to sync or regenerate model catalogs,
    update pricing, or keep provider model lists current.
metadata:
    scope: project
---

# Update Models (multi-source compilation)

## Language

- In-chat reports follow the user's language.
- Docs/skill text and generated comments stay English.

## Purpose

Regenerate `crates/elph-ai/models/*.json` (+ `models/index.json`) by **compiling several
authoritative sources** and resolving each field by a fixed precedence. The JSON files are the
only catalog source: `crates/elph-ai/build.rs` compresses them into the binary on the next build,
so there is no Rust catalog file to regenerate.

### Sources (compiled, in precedence order for each field)

1. **Official provider APIs (live, with optional env-key auth)** — OpenAI-compatible `/models` endpoints
   for providers that expose a `live_pricing_base` (OpenAI, xAI, Mistral, Hyper, Infron, Kilo,
   OpenRouter, OpenCode `https://opencode.ai/zen/v1/models`, OpenCode Go `https://opencode.ai/zen/go/v1/models`, …).
   These return live **pricing**, **model lists**, and, where exposed, **thinking/reasoning capability**.
2. **models.dev `api.json`** — nested `provider → {models}`. Authoritative for `cost`,
   `reasoning_options`, `modalities`, `limit`.
3. **models.dev `models.json` / `catalog.json`** — flat `provider/modelid` index. Authoritative
   for `description`, `knowledge` (cutoff), `benchmarks`, `release_date`, `weights`. No pricing.
4. **OpenRouter `/api/v1/models`** — used both as a live provider API (item 1) and as the
   canonical source for `reasoning.supported_efforts` (thinking levels). Requires
   `OPENROUTER_API_KEY`. Prices are per-token strings converted to per-million.
5. **Nara Router `/api/pricing`** (`https://router.bynara.id/api/pricing`) — Nara's `/v1/models`
   exposes **no pricing**, so this dedicated endpoint is the authoritative source for Nara model
   costs. Uses `official_in_usd_m` / `official_out_usd_m` (USD per million tokens, matching the
   catalog unit). Credit-based fields (`input_credit_per_1k`, …) are intentionally ignored because
   the credit→USD rate is not stable across models. Optional `NARA_API_KEY` is forwarded.
6. **ai-model-directory** (`The-Best-Codes/ai-model-directory`, `data/all.json`) — community
   catalog used as a compiled fallback for **pricing** when neither a live API nor models.dev has a
   price for a model. It can also fill the `reasoning` boolean only when models.dev has no opinion
   on that model. Public, no key required.

Each model always carries a complete `thinkingLevelMap` (7 keys: `off|minimal|low|medium|high|xhigh|max`,
`null` = unsupported). Pricing is
always resolved (never left zero unless every source agrees it is free).

## When to run

- `/update-models`
- "refresh model catalog", "sync models.dev", "regenerate models", "update pricing", "improve catalog accuracy"

## Prerequisites

- Network access (unless `--offline` with existing caches under `models/.cache/`)
- Optional env keys for live pricing/capability probes (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`,
  `HYPER_API_KEY`, `INFRON_API_KEY`, `KILO_API_KEY`, `XAI_API_KEY`, `ANTHROPIC_API_KEY`, …)
- `ai-model-directory` needs no key (fetched from raw GitHub); an offline cache is reused when present
- No local pi clone required for chat catalogs

## Workflow

1. **Generate chat catalogs**

```sh
cargo run -p elph-ai --bin generate-models -- chat && make fmt
# or
make generate-models ARGS="chat" && make fmt
```

Useful flags:

| Flag                | Effect                                                              |
| ------------------- | ------------------------------------------------------------------- |
| `--offline`         | Use cached snapshots only (api/models/catalog + ai-model-directory) |
| `--no-live-pricing` | Skip provider `/models` pricing/capability probes                   |
| `--force`           | Bypass the 24h cache freshness check (always re-fetch)              |

`thinkingLevelMap` is resolved automatically inside `thinking_map.rs` per the precedence in
**Thinking level sourcing** below — not a separate manual step.

2. **Optional full pass** (chat + image fixture path)

```sh
cargo run -p elph-ai --bin generate-models -- all --no-live-pricing
```

3. **Rebuild the binary**

`build.rs` compresses `models/*.json` into the binary at compile time, so the new catalogs only
ship after a rebuild:

```sh
cargo build --release -p elph        # ships compressed catalogs into the single binary
```

4. **Verify**

```sh
cargo test -p elph-ai --test providers --lib models
cargo check -p elph -p elph-ai
```

Confirm:

- `thinkingLevelMap: complete=N incomplete=0` — every model has all 7 keys (incomplete=0 is enforced; the run fails otherwise).
- `thinkingLevelMap source: live-api=X models.dev=Y provider-override=Z previous=W unresolved=V` —
  `unresolved` is expected for **non-reasoning** models (all-null map). For any **reasoning=true**
  model it must be 0, or explicitly reported to the user (never silently filled).
- `cost source: live-api=A models.dev=B ai-model-directory=C previous=D` — `none` should be 0 (no model left unpriced unless genuinely free across all sources).
- `Verified … catalog providers are registered in builtin_providers()`
- Spot-check `anthropic.json`, `xai.json`, a gateway (`openrouter` / `kilo` / `hyper` / `infron`) —
  for at least one reasoning model per file, confirm `thinkingLevelMap` matches the OpenRouter
  `supported_efforts` (or the live API / models.dev entry) it was derived from, not a guess.
- Confirm `description`, `knowledgeCutoff`, and `releaseDate` are present on non-gateway models
  (enriched from models.json / catalog.json).

5. **Summarize for the user**

- Provider count / model count
- `cost` source breakdown (live-api vs models.dev vs ai-model-directory vs previous vs none)
- Any models where thinking support could not be confirmed from any source (report explicitly — do not silently mark as `off` or duplicate a sibling model's map)
- Any providers skipped (not on models.dev and no previous overlay)
- Remaining zero-priced models if any (normally none)
- Reminder: do not hand-edit generated catalogs except intentional Elph overlays; re-run this skill after changes

## Thinking level sourcing

`thinkingLevelMap` must reflect what each model **actually** supports, never a static/hardcoded
table applied across a provider or family. Resolve per model, in this order:

1. **Live provider API (primary, checked even for non-reasoning models)** — if the provider's
   `/models` endpoint exposes `reasoning.supported_efforts` for that exact model id, use it directly.
   This is checked **before** the `reasoning` boolean guard, so an OpenRouter `supported_efforts`
   array still wins even when models.dev lists the model as non-reasoning. Map the provider's native
   levels (`low/medium/high`, `minimal/low/medium/high`, boolean `reasoning: true`) onto the 7-key
   schema; do not invent intermediate levels the provider doesn't expose — mark them `null`.
   Supported effort aliases: `"none"` → `"off"`, `"min"` → `"minimal"`, `"default"` → `"medium"`.
   **OpenRouter `supported_efforts` is the canonical signal here** (`reasoning.supported_efforts`,
   with `default_effort`/`mandatory`/`default_enabled` as supporting context).

2. **models.dev (secondary)** — if the live API doesn't expose per-model thinking capability, look up
   the **same model id** in the cached models.dev entry and use its `reasoning_options` metadata if
   present. Extract `effort`-type options and map their `values` array onto the 7-key schema.

3. **Provider-family override (tertiary)** — when neither live API nor models.dev has data, fall back
   to known provider defaults from official documentation:
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

The `reasoning` boolean prefers models.dev as authoritative. ai-model-directory
`features.reasoning` fills the flag in only when models.dev has no opinion on that model — preventing
false-positive `reasoning` flags on gateway models (e.g. `gpt-4o`) that ai-model-directory mislabels
as reasoning.

Every resolved entry should be traceable back to (a) a live API response field, (b) a specific
models.dev field, (c) an explicit provider override from official docs, or (d) an explicit Elph
overlay — never to inference from the model name or family.

## Data freshness

The generator keeps model data current through four layers:

1. **models.dev cache** — `models/.cache/models.dev/{api,models,catalog}.json` are reused when younger
   than **24h**. Use `--force` to bypass. On a fetch failure (network or non-2xx), the cached
   snapshot is used as a fallback instead of failing. `models.json` + `catalog.json` are merged into a
   `rich` index (catalog.json wins on key conflict) that backs `description` / `knowledgeCutoff` /
   `releaseDate` (fields `api.json` omits).

2. **Live pricing & capability probes** — for providers with `live_pricing_base` + `live_pricing_env`
   set (and the env key present), `/models` is probed for per-model pricing and, where exposed,
   thinking/reasoning capability. Supported pricing shapes (auto-detected per entry):
    - models.dev style: `metadata.pricing.{input_per_million, output_per_million, cached_input_per_million}`
    - Hyper style: `pricing.{input, output, cache_hit, cache_read}`
    - Infron/OneRouter style: `min_prompt_price` / `min_completion_price`
    - **OpenRouter style: `pricing.{prompt, completion, input_cache_read}` as per-token strings —
      converted to per-million by ×1,000,000**
    - Wafer style: nested `wafer.pricing.*_cents_per_million` → USD per million

    Thinking capabilities are extracted from `reasoning.supported_efforts` (array of effort strings)
    and `reasoning.mandatory` / `reasoning.default_enabled` booleans.

3. **Nara Router official pricing** — `/api/pricing` is fetched (cached under
   `models/.cache/models.dev/nara-pricing.json`) and merged into the `nara-router` live result, which
   `resolve_cost` then picks up with top priority. Only `official_in_usd_m`/`official_out_usd_m` are
   used; credit fields are ignored (unstable rate). Forwarded to the `live-api` cost tally.

4. **ai-model-directory (compiled fallback)** — `data/all.json` is fetched (cached under
   `models/.cache/models.dev/ai-model-directory.json`) and used only when live API + models.dev both
   lack a price for a model. Keyed by `provider/modelid` then bare `modelid`.

5. **Live model list (gateway providers)** — for `gateway_preserve_ids` providers with a live
   `/models` endpoint (such as OpenRouter, OpenCode, OpenCode Go, Hyper, Infron, Kilo, etc.), the
   **live id list replaces the previous catalog ids** (source of truth). New upstream models appear
   automatically; removed ones drop out. When the API exposes `category_type`, only `LLM` entries are
   kept so image/video models never pollute the chat catalog. If no live endpoint/key is available,
   the previous catalog ids are preserved.

## Rules

- **Origins**: models.dev (+ its three endpoints) + OpenRouter live API + Nara /api/pricing + ai-model-directory. Never seed chat catalogs from pi.
- **Pricing precedence**: live API (when available) → models.dev → ai-model-directory → keep previous non-zero. `none` (zero from all sources) is only correct for genuinely free models.
- **Every model** must have `thinkingLevelMap` with keys `off|minimal|low|medium|high|xhigh|max` (null = unsupported), resolved per **Thinking level sourcing** — OpenRouter `supported_efforts` (live-api) → models.dev → provider override → preserved overlay → explicit unresolved report. Never invented, never copied from a sibling model.
- **Do not** invent `api` / `baseUrl` from models.dev; preserve Elph factory overlays.
- **Do not** remove Elph-only gateway providers; preserve their model ids and enrich.
- New providers still need a factory in `src/providers/builtin.rs` (generator fails if unregistered).
- Schema contract for user overrides: `schemas/provider-schema.json`.
- **Never** hand-write a Rust catalog module — `models/*.json` is the source, `build.rs` embeds it.

## Code map

| Path                                      | Role                                                                                                                                                                                                                       |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bin/generate_models/main.rs`             | CLI                                                                                                                                                                                                                        |
| `bin/generate_models/models_dev.rs`       | Fetch/cache/merge models.dev `api` + `models` + `catalog` → `ModelsDevData` (api tree + rich index); 24h TTL + fallback. Includes `find_model_by_keyword` for cross-provider family matching.                              |
| `bin/generate_models/provider_sources.rs` | Elph ↔ models.dev key map + live endpoint config                                                                                                                                                                           |
| `bin/generate_models/normalize.rs`        | Entry merge + cost fields + `description`/`knowledgeCutoff`/`releaseDate` enrichment                                                                                                                                       |
| `bin/generate_models/thinking_map.rs`     | Resolves thinkingLevelMap per model: live-api (OpenRouter `supported_efforts`, checked before the `reasoning` guard) → models.dev → provider-override → preserved overlay → unresolved (source-tagged, no invented values) |
| `bin/generate_models/pricing.rs`          | Live pricing probes (incl. OpenRouter per-token ×1e6) + live model id refresh + thinking extraction + Nara /api/pricing + ai-model-directory fallback (`AIModelDir`)                                                       |
| `bin/generate_models/chat.rs`             | Orchestration + registration check + cost source breakdown                                                                                                                                                                 |
| `models/*.json`                           | Catalog source (compressed into the binary)                                                                                                                                                                                |
| `build.rs`                                | zstd frames + provider index for the binary                                                                                                                                                                                |
| `src/models/catalog.rs`                   | Lazy loader (seed + CONFIG_DIR overlay)                                                                                                                                                                                    |
| `schemas/provider-schema.json`            | User override schema (merged by install_provider_catalog_dir)                                                                                                                                                              |

## Do not

- Commit without the user asking
- Run live pricing/capability probes against unpaid keys in a loop
- Hand-edit hundreds of models when a generator fix exists
- Fill `thinkingLevelMap` by guessing from model name/family when no source (live API, models.dev, or Elph overlay) confirms it
- Preserve incorrect previous maps when fresh source data is available — fresh data always takes priority over stale catalog entries
