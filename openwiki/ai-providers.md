# AI Providers

The `elph-ai` crate (`/crates/elph-ai/`) provides a unified LLM API layer with model catalog management, provider abstraction, automatic auth resolution, token/cost tracking, and session context hand-off between models.

Ported from `@earendil-works/pi-ai`.

## Architecture

```
Application code
       ↓
   Models (catalog) ← Providers (registry)
       ↓                      ↓
  ProviderApi ───────→ API implementations
       ↓
  Auth (API keys / OAuth)
```

## Model Catalog

**File**: `/crates/elph-ai/src/models/`

The model catalog manages provider discovery and model selection:

| Type            | Purpose                                                  |
| --------------- | -------------------------------------------------------- |
| `Models`        | The model catalog — holds all providers and their models |
| `Provider`      | A single provider with its API and model list            |
| `ProviderApi`   | The actual API client for making requests                |
| `MutableModels` | Thread-safe wrapper for runtime model changes            |

```rust
let models = builtin_models(None);
let model = models.get_model("anthropic", "claude-sonnet-4-20250514")?;
```

Key files:

- `/crates/elph-ai/src/models/collection.rs` — `Models` struct, provider management
- `/crates/elph-ai/src/models/catalog.rs` — `builtin_models()`, `get_builtin_model()`
- `/crates/elph-ai/src/providers/` — Provider implementations

## Provider API Implementations

**File**: `/crates/elph-ai/src/api/`

Each provider has a dedicated API implementation:

| Provider        | File                         | Auth             |
| --------------- | ---------------------------- | ---------------- |
| OpenAI          | `openai_responses.rs`        | API key or OAuth |
| Azure OpenAI    | `azure_openai_responses.rs`  | API key          |
| AWS Bedrock     | `bedrock_converse_stream.rs` | AWS credentials  |
| Google (Gemini) | `google.rs`                  | API key          |
| HTTP proxy      | `http_proxy.rs`              | Configurable     |

Additional files:

- `openai_completions.rs` — Legacy completions API
- `openai_responses_shared.rs` — Shared OpenAI response types
- `faux.rs` (in `providers/`) — Fake provider for testing

### Transport

Models can use different transports:

- `Transport::Sse` — Server-Sent Events (default for streaming)
- `Transport::CodexWebSocket` — OpenAI Codex WebSocket transport

**File**: `/crates/elph-ai/src/api/codex_transport.rs`

## Authentication

**File**: `/crates/elph-ai/src/auth/`

### API Key Auth

| Type               | Purpose                                  |
| ------------------ | ---------------------------------------- |
| `ApiKeyAuth`       | Key-value API key storage                |
| `env_api_key_auth` | Load API keys from environment variables |
| `ApiKeyCredential` | A single API key entry                   |

### OAuth

Supports OAuth 2.0 for providers like OpenAI (Codex) and GitHub Copilot.

| Function                     | Purpose                                         |
| ---------------------------- | ----------------------------------------------- |
| `openai_codex_oauth`         | OpenAI Codex OAuth flow (device code + browser) |
| `github_copilot_oauth`       | GitHub Copilot OAuth flow                       |
| `anthropic_oauth`            | Anthropic OAuth flow                            |
| `builtin_oauth_provider_ids` | List of built-in OAuth providers                |

**File**: `/crates/elph-ai/src/auth/oauth/`

OAuth flows support:

- Device code login
- Browser-based login
- Token refresh
- Provider registration/unregistration

### Auth Context

`AuthContext` / `DefaultAuthContext` — manages credential resolution across providers.

## Image Generation

**File**: `/crates/elph-ai/src/images/`

Supports generating images through provider APIs:

| Type                    | Purpose                            |
| ----------------------- | ---------------------------------- |
| `ImagesModels`          | Catalog of image generation models |
| `builtin_images_models` | Default set of image models        |
| `generate_images`       | Generate images via provider API   |

## Faux Provider (Testing)

**File**: `/crates/elph-ai/src/providers/faux.rs`

A fake provider for integration testing:

```rust
use elph_ai::{faux_provider, faux_assistant_message, faux_text, faux_tool_call};

let handle = faux_provider(RegisterFauxProviderOptions {
    model_id: "test-model".into(),
    responses: vec![
        FauxResponseStep::Delta(faux_assistant_message("Hello")),
        FauxResponseStep::Delta(faux_tool_call("read", json!({"path": "foo.txt"}))),
    ],
});
```

## Web Tools

**File**: `/crates/elph-agent/src/tools/web/`

Web search and fetch tools, shared between `elph-agent` and `elph-ai`:

| Tool         | Purpose                                       |
| ------------ | --------------------------------------------- |
| `web_search` | Search the web using configured search engine |
| `web_fetch`  | Fetch and extract content from a URL          |

Supports optional [Obscura](https://github.com/h4ckf0r0day/obscura) headless browser for JS-rendered content.

## Types

**File**: `/crates/elph-ai/src/types/`

Core AI types used throughout the workspace:

| Category | Key types                                                   |
| -------- | ----------------------------------------------------------- |
| Messages | `Message`, `UserContent`, `AssistantContent`, `ToolContent` |
| Tools    | `Tool`, `ToolCall`, `ToolResult`                            |
| Events   | `AssistantMessageEvent`, `ProviderStreamEvent`              |
| Thinking | `ThinkingBudgets`, `ThinkingLevel`                          |
| Cost     | `TokenCount`, `CostRecord`                                  |

## Key source files

| Concern             | Path                                                         |
| ------------------- | ------------------------------------------------------------ |
| Models catalog      | `/crates/elph-ai/src/models/catalog.rs`                      |
| Provider collection | `/crates/elph-ai/src/models/collection.rs`                   |
| Provider API trait  | `/crates/elph-ai/src/providers/mod.rs`                       |
| OpenAI API          | `/crates/elph-ai/src/api/openai_responses.rs`                |
| Azure OpenAI API    | `/crates/elph-ai/src/api/azure_openai_responses.rs`          |
| Bedrock API         | `/crates/elph-ai/src/api/bedrock_converse_stream.rs`         |
| Google API          | `/crates/elph-ai/src/api/google.rs` (may be in provider dir) |
| HTTP proxy          | `/crates/elph-ai/src/api/http_proxy.rs`                      |
| Codex WebSocket     | `/crates/elph-ai/src/api/codex_transport.rs`                 |
| API key auth        | `/crates/elph-ai/src/auth/mod.rs`                            |
| OAuth               | `/crates/elph-ai/src/auth/oauth/`                            |
| Auth context        | `/crates/elph-ai/src/auth/mod.rs` (within)                   |
| Image gen           | `/crates/elph-ai/src/images/`                                |
| Faux provider       | `/crates/elph-ai/src/providers/faux.rs`                      |
| Core types          | `/crates/elph-ai/src/types/`                                 |
| Session resources   | `/crates/elph-ai/src/session_resources.rs`                   |
| Deferred tools      | `/crates/elph-ai/src/utils/deferred_tools.rs`                |

## Change guidance

- **Adding a provider**: Add API implementation in `api/`, register in `providers/`, add model definitions.
- **Modifying auth**: Check all provider API files for credential handling.
- **New model catalog entry**: Add to `models/catalog.rs` or the provider-specific initialization.
- **Web tool changes**: Web tools span both `elph-ai` and `elph-agent`.
- **Tests**: Provider tests in `/crates/elph-ai/tests/` — check `bedrock_*.rs`, `openai_*.rs`, `google_*.rs`, `oauth_auth.rs`, `http_proxy.rs`.
- **Model generation**: `make generate-models` regenerates catalogs from a catalog source.
