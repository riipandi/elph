# Porting status: pi-ai → elph-ai

| Field               | Value                                                              |
| ------------------- | ------------------------------------------------------------------ |
| **Last audited**    | 2026-07-11T11:12:19Z                                               |
| **Upstream**        | `@earendil-works/pi-ai` · `packages/ai` · **v0.80.6** + Unreleased |
| **Upstream commit** | `4c18610` (2026-07-11)                                             |
| **Elph crate**      | `crates/elph-ai`                                                   |
| **Primary sources** | pi `CHANGELOG.md` / `src/**`; elph `src/**` + `models/**`          |

---

## At a glance

| Area                                                  | Assessment                                                            |
| ----------------------------------------------------- | --------------------------------------------------------------------- |
| Architecture (`Models`, providers, auth, stream APIs) | **Mostly parity** with 0.80.x design                                  |
| Model catalog freshness                               | **Partial** — missing GPT-5.6 family, cost tiers, `max` thinking maps |
| Thinking levels                                       | **Partial** — has `xhigh`, missing `max`                              |
| Deferred / dynamic tools                              | **Missing in elph** (Unreleased on pi)                                |
| Cost accounting (long-context tiers)                  | **Missing in elph**                                                   |
| Provider transports (SSE, Codex WS, zstd, Bedrock)    | **Mostly parity**; Bedrock `apiKey` path **partial**                  |
| Diagnostics / session resource cleanup                | **Missing in elph**                                                   |
| Hyper provider                                        | **Missing in pi** (Elph-only)                                         |

---

## Audit log

| Timestamp (UTC)      | Pi version / commit               | Notes                                                                                             |
| -------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------- |
| 2026-07-11T11:12:19Z | `0.80.6` + Unreleased @ `4c18610` | Initial gap audit after local clone + tree compare. Catalog counts: elph ~756 model ids, pi ~760. |

Add a new row on every re-audit; do not delete old rows.

---

## Module map

| pi (`packages/ai/src`)                                     | elph (`crates/elph-ai/src`)           | Status                                                                                     |
| ---------------------------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------ |
| `api/*` (providers stream impls)                           | `api/*`                               | **Parity** (plus Rust helpers: `sse`, `websocket_connect`, `azure_base_url`, `http_proxy`) |
| `auth/*`                                                   | `auth/*`                              | **Parity**                                                                                 |
| `utils/oauth/*`                                            | `auth/oauth/*`                        | **Parity**                                                                                 |
| `models.ts` + collection                                   | `models/`                             | **Partial** (cost tiers, thinking clamp)                                                   |
| `providers/*` + generated models                           | `providers/` + `models/*.json`        | **Partial** (catalog content lag)                                                          |
| `types.ts`                                                 | `types/`                              | **Partial** (see type gaps)                                                                |
| `images*`, `images-models`                                 | `images/`                             | **Parity**                                                                                 |
| `utils/estimate.ts`                                        | `utils/estimate.rs`                   | **Partial** (elph is char heuristic only)                                                  |
| `utils/overflow`, `retry`, `validation`, `event-stream`, … | `utils/*`                             | **Parity** (shape differs)                                                                 |
| `utils/deferred-tools.ts`                                  | —                                     | **Missing in elph**                                                                        |
| `utils/diagnostics.ts`                                     | —                                     | **Missing in elph**                                                                        |
| `session-resources.ts`                                     | —                                     | **Missing in elph**                                                                        |
| `utils/abort-signals.ts`                                   | `CancellationToken` in stream options | **N/A** (Rust equivalent)                                                                  |
| `*.lazy.ts` API splitting                                  | single modules                        | **N/A** (bundler concern)                                                                  |
| `compat.ts` legacy aliases                                 | —                                     | **N/A** / intentionally skipped                                                            |
| —                                                          | Hyper provider + `models/hyper.json`  | **Missing in pi**                                                                          |

---

## Types and public contract

### Thinking levels

| Capability                                  | pi  | elph                                           | Status                                            |
| ------------------------------------------- | --- | ---------------------------------------------- | ------------------------------------------------- |
| `minimal` … `high`                          | yes | yes                                            | **Parity**                                        |
| `xhigh`                                     | yes | yes (`ThinkingLevel::Xhigh`)                   | **Parity**                                        |
| `max` (0.80.6)                              | yes | no                                             | **Missing in elph**                               |
| Per-model `thinkingLevelMap`                | yes | yes (field present)                            | **Partial** — catalog often lacks `"max"` entries |
| Anthropic effort `"max"` / native `"xhigh"` | yes | `xhigh` only; empty-text thinking handling lag | **Partial**                                       |

### Usage and cost

| Capability                                      | pi  | elph                   | Status              |
| ----------------------------------------------- | --- | ---------------------- | ------------------- |
| `Usage` input/output/cache/total + cost         | yes | yes                    | **Parity**          |
| `cacheWrite1h` + 2× input pricing               | yes | yes (`cache_write_1h`) | **Parity**          |
| `Usage.reasoning` (0.80.3)                      | yes | yes                    | **Parity**          |
| `ModelCost.tiers` / `inputTokensAbove` (0.80.6) | yes | no                     | **Missing in elph** |
| `calculateCost` tier selection                  | yes | flat rates only        | **Missing in elph** |
| OpenAI service-tier multipliers on stream       | yes | yes (Responses/Codex)  | **Parity**          |

### Messages and tools

| Capability                                      | pi        | elph                        | Status              |
| ----------------------------------------------- | --------- | --------------------------- | ------------------- |
| User / Assistant / ToolResult messages          | yes       | yes                         | **Parity**          |
| `ToolResultMessage.addedToolNames` (Unreleased) | yes       | no                          | **Missing in elph** |
| `splitDeferredTools` + native deferred load     | yes       | no                          | **Missing in elph** |
| Anthropic `tool_reference` for late tools       | yes       | no                          | **Missing in elph** |
| OpenAI Responses `deferLoading` / tool search   | yes       | no                          | **Missing in elph** |
| Compat: `supportsToolSearch` / deferred flags   | yes       | no                          | **Missing in elph** |
| `AssistantMessage.diagnostics`                  | yes       | no                          | **Missing in elph** |
| Thinking / text / toolCall content blocks       | yes       | yes                         | **Parity**          |
| Empty thinking text + valid signature (#6457)   | preserved | dropped (`is_empty` filter) | **Missing in elph** |

### Models collection / auth

| Capability                               | pi  | elph                | Status                                      |
| ---------------------------------------- | --- | ------------------- | ------------------------------------------- |
| `Provider` + `Models` collection         | yes | yes                 | **Parity**                                  |
| `createModels` / `builtinModels`         | yes | yes                 | **Parity**                                  |
| Credential store + resolve auth          | yes | yes                 | **Parity**                                  |
| OAuth (Anthropic, Codex, Copilot)        | yes | yes                 | **Parity**                                  |
| `ApiKeyCredential` `type: "api_key"`     | yes | yes (serde shapes)  | **Parity** (verify if re-syncing auth.json) |
| Request-scoped `apiKey`/`env` in resolve | yes | yes (StreamOptions) | **Partial** on Bedrock (see below)          |
| Faux provider for tests                  | yes | yes                 | **Parity**                                  |

---

## Provider APIs

| API / feature                                           | pi                                 | elph                                                  | Status                                                     |
| ------------------------------------------------------- | ---------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------- |
| Anthropic Messages                                      | yes                                | yes                                                   | **Partial** (deferred tools, empty thinking, `max` effort) |
| OpenAI Completions                                      | yes                                | yes                                                   | **Parity** (major paths)                                   |
| OpenAI Responses                                        | yes                                | yes                                                   | **Partial** (deferred tools)                               |
| OpenAI Codex Responses (SSE + WS)                       | yes                                | yes                                                   | **Parity** on transport; deferred tools lag                |
| Codex request body **zstd**                             | yes                                | yes (`codex_transport`)                               | **Parity**                                                 |
| Azure OpenAI Responses / Foundry URLs                   | yes                                | yes                                                   | **Parity**                                                 |
| Google Generative AI + Vertex                           | yes                                | yes                                                   | **Parity**                                                 |
| Bedrock Converse Stream (SigV4 + bearer)                | yes                                | yes                                                   | **Partial**                                                |
| Bedrock: generic stream `apiKey` as bearer (Unreleased) | `apiKey \|\| bearerToken \|\| env` | `bearer_token \|\| AWS_BEARER_TOKEN_BEDROCK` only     | **Missing in elph**                                        |
| Mistral Conversations                                   | yes                                | yes                                                   | **Parity**                                                 |
| Cloudflare / gateway helpers                            | yes                                | yes                                                   | **Parity**                                                 |
| OpenRouter images                                       | yes                                | yes                                                   | **Parity**                                                 |
| GitHub Copilot headers / device code                    | yes                                | yes                                                   | **Partial** (re-check 0.80.4 device-code timing fixes)     |
| `transform-messages` null content normalize (#6343)     | yes                                | tool-id normalize only; typed content less null-prone | **Partial** / lower risk in Rust                           |

---

## Model catalog (`models/*.json` vs generated TS)

Snapshot at audit time:

| Item                                               | elph              | pi               | Status                                    |
| -------------------------------------------------- | ----------------- | ---------------- | ----------------------------------------- |
| Approx. model id count                             | ~756              | ~760             | **Parity** (count)                        |
| GPT-5.4 / GPT-5.5 family                           | present           | present          | **Parity**                                |
| GPT-5.6 sol / terra / luna (0.80.4+)               | **absent**        | present          | **Missing in elph**                       |
| Claude Sonnet 5 / Fable 5                          | present           | present          | **Partial** (thinking maps incomplete)    |
| `cost.tiers` / `inputTokensAbove` entries          | **0**             | ~12+             | **Missing in elph**                       |
| `thinkingLevelMap` including `"max"`               | rare / incomplete | widespread (~97) | **Missing in elph**                       |
| Copilot extended `contextWindow: 1000000` (0.80.4) | some models       | yes              | **Partial** — re-verify all listed models |
| Hyper provider catalog                             | yes               | no               | **Missing in pi**                         |

Regenerate via elph `bin/generate_models` after pulling upstream generation scripts / models.dev refresh.

---

## Utilities

| Utility                                     | pi              | elph                      | Status                                                                                |
| ------------------------------------------- | --------------- | ------------------------- | ------------------------------------------------------------------------------------- |
| Context estimate (usage + trailing + tools) | sophisticated   | char/4 only in `elph-ai`  | **Missing in elph** (agent crate has better estimate)                                 |
| Post-compaction usage invalidation (#6464)  | timestamp-aware | not in `elph-ai` estimate | **Missing in elph**                                                                   |
| Overflow detection                          | yes             | yes                       | **Parity** (watch new error shapes)                                                   |
| Retry classification                        | yes             | yes                       | **Partial** — re-check gRPC ResourceExhausted, CF 524, Bun socket strings if relevant |
| Session resource cleanup registry           | yes             | no                        | **Missing in elph**                                                                   |
| Redacted diagnostics on assistant messages  | yes             | no                        | **Missing in elph**                                                                   |

---

## Prioritized gaps (port worklist)

### P0 — do first

1. **`ThinkingLevel::Max`** end-to-end (types, clamp, Anthropic/Bedrock/OpenAI maps, agent enum if shared).
2. **`ModelCost.tiers` + `calculate_cost`** tier selection.
3. **Regenerate model catalogs** (GPT-5.6, tiers, `thinkingLevelMap.max`, Copilot context windows).
4. **Deferred tools pipeline**: `added_tool_names`, `split_deferred_tools`, Anthropic + OpenAI Responses/Codex wiring, compat flags.

### P1 — correctness

5. Preserve Anthropic thinking blocks with empty text + valid signature (#6457).
6. Bedrock: use `StreamOptions.api_key` as bearer token (Unreleased).
7. Align context estimation with pi (`estimate.ts` semantics), including compaction boundary.

### P2 — polish

8. `AssistantMessage.diagnostics` + diagnostics helpers.
9. Session resource cleanup registry (if multi-session long-lived processes need it).
10. Re-audit OAuth device-code timing and overflow error strings after catalog sync.

### Skip / N/A

- Lazy `.lazy.ts` API modules
- `compat` legacy stream alias entrypoint
- TypeBox-specific helpers (use `schemars` / serde in Rust)

---

## Intentionally not mirrored

| pi concern                                       | Reason                                            |
| ------------------------------------------------ | ------------------------------------------------- |
| Node/Bun-only zlib zstd discovery                | elph links `zstd` crate directly                  |
| Package export map (`./compat`, `./providers/*`) | Cargo features / modules instead                  |
| models.dev generation living inside package      | elph uses `models/*.json` + `bin/generate_models` |

---

## How to re-audit this package

```bash
# Upstream
cd /path/to/pi && git pull && git rev-parse --short HEAD
head -80 packages/ai/CHANGELOG.md

# Compare surfaces
# - types: ThinkingLevel, ModelCost, ToolResultMessage, Usage
# - utils: deferred-tools, diagnostics, estimate, session-resources
# - models: id sets, tiers, thinkingLevelMap.max
# - api: anthropic empty thinking; bedrock apiKey; responses deferred tools
```

Then update: **Last audited**, **Audit log**, and any status cells that changed.
