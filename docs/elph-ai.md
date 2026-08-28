# elph-ai consumer contract

`elph-ai` is the standalone LLM client crate. Hosts (including Elph) depend on the surface below. Adapter internals and `#[doc(hidden)]` items are not stable.

Version: `0.0.28`. MSRV: **1.88** (edition 2024). Generation errors stay **in-band** (`AssistantMessageEvent::Error`, `StopReason::Error` / `Aborted`). They are not `Result`.

Docs: <https://docs.rs/elph-ai>

## Public modules

| Path | Stable for |
| --- | --- |
| crate root | Prelude types, `Models` / `builtin_models`, faux, images, `validate_tool_call`, `estimate`, `ClientIdentity` |
| `elph_ai::types` | Messages, models, stream options, identity |
| `elph_ai::models` | Collection, catalog install/update |
| `elph_ai::providers` | Built-in factories and `builtin_models` |
| `elph_ai::auth` | Credential stores, `ModelsError`, OAuth login/refresh |
| `elph_ai::api` | Re-exported API impl types (`AnthropicMessagesApi`, `builtin_apis`, `wrap_on_payload`) |
| `elph_ai::images` | Image generation |
| `elph_ai::resilience` | Rate limits / circuit breaker |
| `elph_ai::estimate` | Token estimate (`count_tokens_text`) |

`elph_ai::utils` and `elph_ai::trace` are `#[doc(hidden)]` and are not part of the contract.

## Errors

Out-of-band APIs return `Result<T, ModelsError>`:

- `Models::refresh`, `Models::get_auth`
- `resolve_provider_auth`
- catalog parse / provider update
- `oauth_provider_login`, `refresh_oauth_token`, `get_oauth_api_key`, `oauth_provider_to_auth`

`ModelsError` has `code`, `message`, and optional `source: Box<dyn Error + Send + Sync>` — not `anyhow::Error` in the public type.

## Host identity

Set [`ClientIdentity`](../crates/elph-ai/src/types/mod.rs) on `CreateModelsOptions` (stored on that collection). Pass the same identity to `oauth_provider_login(..., identity)` and `ResilienceManager::with_env_prefix`. Two collections can use different prefixes; nothing is process-global.

| Field | Effect |
| --- | --- |
| `product` | Codex `originator`, xAI `referrer` |
| `env_prefix` | `{PREFIX}_CACHE_RETENTION`, `{PREFIX}_GITHUB_HOST`, `{PREFIX}_RATE_LIMIT_*`, `{PREFIX}_CIRCUIT_BREAKER_*`, `{PREFIX}_MAX_RETRIES` |

Default is `product = "elph"`, `env_prefix = "ELPH"`. Elph sets this explicitly when building `Models`.

`StreamOptions.cache_retention` controls provider-managed prompt-prefix caching.
Use `CacheRetention::None`, `Short`, or `Long`; an explicit request value takes
precedence over `{PREFIX}_CACHE_RETENTION`, then the default is `Short`.
`{PREFIX}_CACHE_RETENTION` accepts `none`, `short`, and `long`; invalid values
fall back to `short` with one process-level warning. `session_id` is used as an
opaque provider affinity key for cache-enabled requests and is not sent as cache
affinity when retention is `None`.

See [Context caching](./context-caching.md) for the provider mapping, workload
policy, usage accounting, and troubleshooting guidance.

OAuth browser callback requires the `oauth-callback` feature.

## Cargo features

| Feature | Default | Purpose |
| --- | --- | --- |
| *(none)* | — | HTTP chat APIs, catalogs, faux, images HTTP |
| `bedrock` | off | AWS Bedrock SDK + `amazon_bedrock_provider` |
| `oauth-callback` | off | Local Axum server for browser OAuth |
| `generate-models` | off | `generate-models` binary + clap |
| `tracing` | off | fastrace |

The Elph binary enables `tracing`, `bedrock`, and `oauth-callback`.

## What is not public

- Codex WebSocket debug helpers at crate root (use `elph_ai::api::codex_transport`)
- The OAuth function forest at crate root (use `elph_ai::auth`)
- `pub use anyhow::Result`
- Per-provider compile features (`openai`, `anthropic`, …)
