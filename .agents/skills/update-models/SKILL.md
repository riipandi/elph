---
name: update-models
description: >-
    Refresh elph-ai chat model catalogs from models.dev (origin) with optional live
    provider pricing, inject full thinkingLevelMap on every model, and verify
    provider registration. Use when the user runs
    /update-models, asks to sync or regenerate model catalogs, update models.dev
    data, refresh pricing, or keep provider model lists current.
---

# Update Models (models.dev origin)

## Language

- In-chat reports follow the user's language.
- Docs/skill text and generated comments stay English.

## Purpose

Regenerate `crates/elph-ai/models/*.json` (+ `models/index.json`) from **models.dev** as the sole catalog origin.
Pricing prefers live provider APIs when keys/endpoints allow, then models.dev. Every model must include a complete `thinkingLevelMap`.

The JSON files are the only catalog source: `crates/elph-ai/build.rs` compresses them into the binary on the next build, so there is no Rust catalog file to regenerate.

## When to run

- `/update-models`
- "refresh model catalog", "sync models.dev", "regenerate models", "update pricing"

## Prerequisites

- Network access (unless `--offline` with existing `models/.cache/models.dev/api.json`)
- Optional env keys for live pricing probes (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `HYPER_API_KEY`, `INFRON_API_KEY`, …)
- No local pi clone required for chat catalogs

## Workflow

1. **Generate chat catalogs**

```sh
cargo run -p elph-ai --bin generate-models -- chat
# or
make generate-models ARGS="chat"
```

Useful flags:

| Flag                | Effect                                                        |
| ------------------- | ------------------------------------------------------------- |
| `--offline`         | Use cached models.dev only                                    |
| `--no-live-pricing` | Skip provider `/models` pricing probes                        |
| `--force`           | Bypass the models.dev cache freshness check (always re-fetch) |

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
- `Verified … catalog providers are registered in builtin_providers()`
- Spot-check `anthropic.json`, `xai.json`, a gateway (`openrouter` / `kilo` / `hyper` / `infron`)

5. **Summarize for the user**

- Provider count / model count
- Any providers skipped (not on models.dev and no previous overlay)
- Remaining zero-priced models if any
- Reminder: do not hand-edit generated catalogs except intentional Elph overlays; re-run this skill after changes

## Data freshness

The generator keeps model data current through three layers:

1. **models.dev cache** — `models/.cache/models.dev/api.json` is reused when younger than **24h**. Use `--force` to bypass. On a fetch failure (network or non-2xx), the cached snapshot is used as a fallback instead of failing.

2. **Live pricing** — for providers with `live_pricing_base` + `live_pricing_env` set (and the env key present), `/models` is probed for per-model pricing. Supported shapes:
    - models.dev style: `metadata.pricing.{input_per_million, output_per_million, cached_input_per_million}`
    - Hyper style: `pricing.{input, output, cache_hit, cache_create}`
    - Infron/OneRouter style: `min_prompt_price` / `min_completion_price`

3. **Live model list (gateway providers)** — for `gateway_preserve_ids` providers with a live `/models` endpoint, the **live id list replaces the previous catalog ids** (source of truth). New upstream models appear automatically; removed ones drop out. When the API exposes `category_type`, only `LLM` entries are kept so image/video models never pollute the chat catalog. If no live endpoint/key is available, the previous catalog ids are preserved.

## Rules

- **Origin**: models.dev only — never seed chat catalogs from pi.
- **Pricing**: live API (when available) → models.dev → keep previous non-zero.
- **Every model** must have `thinkingLevelMap` with keys `off|minimal|low|medium|high|xhigh|max` (null = unsupported).
- **Do not** invent `api` / `baseUrl` from models.dev; preserve Elph factory overlays.
- **Do not** remove Elph-only gateway providers; preserve their model ids and enrich.
- New providers still need a factory in `src/providers/builtin.rs` (generator fails if unregistered).
- Schema contract for user overrides: `schemas/provider-schema.json`.
- **Never** hand-write a Rust catalog module — `models/*.json` is the source, `build.rs` embeds it.

## Code map

| Path                                      | Role                                                          |
| ----------------------------------------- | ------------------------------------------------------------- |
| `bin/generate_models/main.rs`             | CLI                                                           |
| `bin/generate_models/models_dev.rs`       | Fetch/cache models.dev (24h TTL + fallback)                   |
| `bin/generate_models/provider_sources.rs` | Elph ↔ models.dev map + live endpoint config                  |
| `bin/generate_models/normalize.rs`        | Entry merge + cost fields                                     |
| `bin/generate_models/thinking_map.rs`     | Full 7-key maps                                               |
| `bin/generate_models/pricing.rs`          | Live pricing probes + live model id refresh                   |
| `bin/generate_models/chat.rs`             | Orchestration + registration check                            |
| `models/*.json`                           | Catalog source (compressed into the binary)                   |
| `build.rs`                                | zstd frames + provider index for the binary                   |
| `src/models/catalog.rs`                   | Lazy loader (seed + CONFIG_DIR overlay)                       |
| `schemas/provider-schema.json`            | User override schema (merged by install_provider_catalog_dir) |

## Do not

- Commit without the user asking
- Run live pricing against unpaid keys in a loop
- Hand-edit hundreds of models when a generator fix exists
