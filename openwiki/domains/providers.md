---
type: Concept
title: Providers — LLM Provider Adapters
description: Elph's provider abstraction layer — 30+ provider adapters, ProviderStreams trait, compat flags, ThinkingLevel, constrainedSampling, SessionAffinityFormat
tags: [providers, llm, adapters, provider-streams, compat-flags]
---

# Providers

The provider layer lives in `crates/elph-ai/src/providers/`. It wraps every supported LLM API behind a unified streaming interface. Provider auth is resolved via [resolve_provider_auth](../workflows/auth.md) which handles API keys and OAuth. See [Architecture Overview](../architecture/overview.md) for how providers connect to the agent harness.

## ProviderStreams Trait

Defined in `crates/elph-ai/src/providers/adapter.rs`:

```rust
pub trait ProviderStreams: Send + Sync {
    fn complete(
        &self,
        request: ModelRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'static, ProviderResult>;
}
```

Each provider adapter implements this trait. The `complete()` method:

1. Transforms the internal `ModelRequest` into the provider's native API format.
2. Streams the response via SSE/WebSocket.
3. Returns a `ProviderResult` with content blocks, stop reason, and usage.

## Built-in Provider Adapters

From `crates/elph-ai/src/providers/builtin.rs` and `adapter.rs`:

| Adapter                         | API Format              | Provider(s)                                                        |
| ------------------------------- | ----------------------- | ------------------------------------------------------------------ |
| `anthropic_messages_api()`      | Anthropic Messages      | Anthropic, Anthropic-compatible gateways                           |
| `openai_completions_api()`      | OpenAI Chat Completions | OpenAI, xAI, Mistral, NeuralWatt, Hyper, Nvidia, OpenGateway, etc. |
| `openai_responses_api()`        | OpenAI Responses API    | OpenAI (newer format)                                              |
| `openai_codex_responses_api()`  | OpenAI Codex Responses  | OpenAI Codex                                                       |
| `azure_openai_responses_api()`  | Azure OpenAI Responses  | Azure OpenAI                                                       |
| `bedrock_converse_stream_api()` | AWS Bedrock Converse    | Amazon Bedrock                                                     |
| `google_generative_ai_api()`    | Google Generative AI    | Google Gemini                                                      |
| `google_vertex_api()`           | Google Vertex AI        | Google Vertex                                                      |
| `mistral_conversations_api()`   | Mistral Conversations   | Mistral                                                            |
| `mixed_gateway_apis()`          | Auto-detect             | Cloudflare AI Gateway                                              |
| `mixed_openai_apis()`           | OpenAI-compatible       | Sumopod, Kilo, etc.                                                |

## Provider Factory Functions

From `crates/elph-ai/src/providers/builtin.rs`:

```rust
pub fn anthropic_provider() -> Provider;
pub fn openai_provider() -> Provider;
pub fn amazon_bedrock_provider() -> Provider;
pub fn google_vertex_provider() -> Provider;
pub fn cloudflare_ai_gateway_provider() -> Provider;
pub fn cloudflare_workers_ai_provider() -> Provider;
pub fn hyper_provider() -> Provider;          // [Elph delta]
pub fn mistral_provider() -> Provider;
pub fn neuralwatt_provider() -> Provider;
pub fn nvidia_provider() -> Provider;
pub fn sumopod_provider() -> Provider;
pub fn xai_provider() -> Provider;
```

Plus additional providers from the catalog: Kilo, OpenGateway, Xiaomi, ZAI, and more (~30+ total).

## Compat Flags

Defined in `crates/elph-ai/src/types/mod.rs`. Each provider declares capabilities via `CompatFlags`:

```rust
pub struct CompatFlags {
    pub supports_tool_search: bool,
    pub supports_tool_references: bool,
    pub supports_openai_grammar_tools: bool,  // Sprint 5
    pub supports_strict_tools: bool,           // Sprint 5
    pub supports_strict_mode: bool,            // Sprint 5
    pub supports_thinking: bool,
    pub supports_extended_thinking: bool,
    pub supports_max_thinking: bool,
    pub supports_streaming: bool,
    pub supports_images: bool,
    pub supports_tool_choice: bool,
    pub supports_tool_result_images: bool,
    pub supports_system_prompt: bool,
    pub supports_temperature: bool,
    pub supports_max_tokens: bool,
    pub supports_prompt_caching: bool,
    // ... and more
}
```

## ThinkingLevel

```rust
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}
```

`clamp_thinking_level()` (from `crates/elph-ai/src/models/mod.rs`) adjusts the level to the provider's supported range.

## constrainedSampling

Added Sprint 5 (commit `f3642ee`):

```rust
pub struct ConstrainedSamplingConfig {
    pub strict_mode: Option<StrictMode>,
    pub grammar: Option<GrammarVariants>,
}

pub enum StrictMode {
    True,
    False,
    Auto,
}

pub enum GrammarVariants {
    JsonSchema(Value),
    BackusNaur(String),
    Regex(String),
}
```

Each `Tool` now carries `constrained_sampling: Option<ConstrainedSamplingConfig>`.

## SessionAffinityFormat

Replaces the old `sendSessionIdHeader` boolean (Sprint 5):

```rust
pub enum SessionAffinityFormat {
    Header(String),
    Cookie(String),
    UrlParam(String),
    None,
}
```

## Provider Catalog

Model catalogs live in `crates/elph-ai/models/*.json` and are loaded by `crates/elph-ai/src/models/catalog.rs`. Regenerated via `make generate-models` (reads from `../../earendil-works/pi/packages/ai`).

## Source References

- `crates/elph-ai/src/providers/builtin.rs` — provider factory functions
- `crates/elph-ai/src/providers/adapter.rs` — `ProviderStreams` trait and adapter implementations
- `crates/elph-ai/src/providers/faux/` — `FauxProviderHandle` for testing
- `crates/elph-ai/src/providers/cloudflare_auth.rs` — Cloudflare-specific auth
- `crates/elph-ai/src/types/mod.rs` — `CompatFlags`, `ThinkingLevel`, `ConstrainedSamplingConfig`, `SessionAffinityFormat`
- `crates/elph-ai/src/models/mod.rs` — `clamp_thinking_level()`, `calculate_cost()`, `create_models()`
