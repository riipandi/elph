# elph-ai consumer contract

`elph-ai` is the standalone LLM client crate. Hosts (including Elph) depend on the surface below. Adapter internals and `#[doc(hidden)]` items are not stable.

Version: `0.0.28`. MSRV: workspace `rust-version` (currently 1.97). Generation errors stay **in-band** (`AssistantMessageEvent::Error`, `StopReason::Error` / `Aborted`). They are not `Result`.

## Public modules

| Path | Stable for |
| --- | --- |
| crate root | Prelude types, `Models` / `builtin_models`, faux, images, `validate_tool_call`, `estimate`, `ClientIdentity` |
| `elph_ai::types` | Messages, models, stream options |
| `elph_ai::models` | Collection, catalog install/update |
| `elph_ai::providers` | Built-in factories and `builtin_models` |
| `elph_ai::auth` | Credential stores, `ModelsError`, OAuth login/registry |
| `elph_ai::api` | Re-exported API impl types (`AnthropicMessagesApi`, `builtin_apis`, `wrap_on_payload`) |
| `elph_ai::images` | Image generation |
| `elph_ai::resilience` | Rate limits / circuit breaker |
| `elph_ai::estimate` | Token estimate (`count_tokens_text`) |

`elph_ai::utils` is `#[doc(hidden)]` and is not part of the contract.

## Errors

Out-of-band APIs return `Result<T, ModelsError>`:

- `Models::refresh`, `Models::get_auth`
- `resolve_provider_auth`
- catalog parse / provider update failures
- OAuth login/refresh (via `anyhow` at the OAuth boundary, wrapped into `ModelsError` on resolve)

`ModelsError` has `code`, `message`, and optional `source: Box<dyn Error + Send + Sync>` — not `anyhow::Error`.

## Host identity

Set [`ClientIdentity`](../crates/elph-ai/src/types/mod.rs) on `CreateModelsOptions`:

- `product` — Codex `originator`, xAI `referrer`
- `env_prefix` — `{PREFIX}_CACHE_RETENTION`, `{PREFIX}_GITHUB_HOST`, `{PREFIX}_RATE_LIMIT_*`, `{PREFIX}_MAX_RETRIES`

Default is `product = "elph"`, `env_prefix = "ELPH"`. Elph sets this explicitly when building `Models`.

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

- Codex WebSocket debug helpers at crate root
- The OAuth function forest at crate root (use `elph_ai::auth`)
- `pub use anyhow::Result`
- Per-provider compile features (`openai`, `anthropic`, …)
