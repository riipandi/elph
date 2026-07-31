---
name: update-models
description: >-
    Refresh elph-ai chat model catalogs from models.dev (origin) with optional live
    provider pricing, inject full thinkingLevelMap on every model, regenerate
    src/models/catalog.rs, and verify registration. Use when the user runs
    /update-models, asks to sync or regenerate model catalogs, update models.dev
    data, refresh pricing, or keep provider model lists current.
---

# Update Models (models.dev origin)

## Language

- In-chat reports follow the user's language.
- Docs/skill text and generated comments stay English.

## Purpose

Regenerate `crates/elph-ai/models/*.json` and `src/models/catalog.rs` from **models.dev** as the sole catalog origin.
Pricing prefers live provider APIs when keys/endpoints allow, then models.dev. Every model must include a complete `thinkingLevelMap`.

## When to run

- `/update-models`
- "refresh model catalog", "sync models.dev", "regenerate models", "update pricing"

## Prerequisites

- Network access (unless `--offline` with existing `models/.cache/models.dev/api.json`)
- Optional env keys for live pricing probes (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, …)
- No local pi clone required for chat catalogs

## Workflow

1. **Generate chat catalogs**

```bash
cargo run -p elph-ai --bin generate-models -- chat
# or
make generate-models ARGS="chat"
```

Useful flags:

| Flag                      | Effect                                 |
| ------------------------- | -------------------------------------- |
| `--offline`               | Use cached models.dev only             |
| `--no-live-pricing`       | Skip provider `/models` pricing probes |
| `--no-regenerate-catalog` | Write JSON only                        |

2. **Optional full pass** (chat + image fixture path)

```bash
cargo run -p elph-ai --bin generate-models -- all --no-live-pricing
```

3. **Verify**

```bash
cargo test -p elph-ai --test providers --lib models
cargo check -p elph -p elph-ai
```

Confirm:

- `thinkingLevelMap: complete=N incomplete=0` in generator output
- `Verified … catalog providers are registered in builtin_providers()`
- Spot-check `anthropic.json`, `xai.json`, a gateway (`openrouter` / `kilo`)

4. **Summarize for the user**

- Provider count / model count
- Any providers skipped (not on models.dev and no previous overlay)
- Remaining zero-priced models if any
- Reminder: do not hand-edit generated catalogs except intentional Elph overlays; re-run this skill after changes

## Rules

- **Origin**: models.dev only — never seed chat catalogs from pi.
- **Pricing**: live API (when available) → models.dev → keep previous non-zero.
- **Every model** must have `thinkingLevelMap` with keys `off|minimal|low|medium|high|xhigh|max` (null = unsupported).
- **Do not** invent `api` / `baseUrl` from models.dev; preserve Elph factory overlays.
- **Do not** remove Elph-only gateway providers; preserve their model ids and enrich.
- New providers still need a factory in `src/providers/builtin.rs` (generator fails if unregistered).
- Schema contract for future user overrides: `schemas/provider-schema.json` (runtime merge not implemented yet).

## Code map

| Path                                      | Role                            |
| ----------------------------------------- | ------------------------------- |
| `bin/generate_models/main.rs`             | CLI                             |
| `bin/generate_models/models_dev.rs`       | Fetch/cache models.dev          |
| `bin/generate_models/provider_sources.rs` | Elph ↔ models.dev map           |
| `bin/generate_models/normalize.rs`        | Entry merge + cost fields       |
| `bin/generate_models/thinking_map.rs`     | Full 7-key maps                 |
| `bin/generate_models/pricing.rs`          | Live pricing probes             |
| `bin/generate_models/chat.rs`             | Orchestration + catalog.rs      |
| `models/*.json`                           | Embedded catalogs               |
| `schemas/provider-schema.json`            | Forward-looking override schema |

## Do not

- Commit without the user asking
- Run live pricing against unpaid keys in a loop
- Hand-edit hundreds of models when a generator fix exists
