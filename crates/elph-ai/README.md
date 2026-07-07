# elph-ai

Unified LLM API with provider collections, automatic auth resolution, token and cost tracking,
and simple context persistence and hand-off to other models mid-session.
Rust port of [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).

**Note**: This library only includes models that support tool calling (function calling), as this is essential for agentic workflows.

## Table of Contents

- [Supported Providers](#supported-providers)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Providers and Models](#providers-and-models)
    - [Provider Factories](#provider-factories)
    - [All Built-in Providers](#all-built-in-providers)
    - [Querying Models](#querying-models)
    - [Static Catalog Reads](#static-catalog-reads)
    - [Dynamic Providers](#dynamic-providers)
- [Auth](#auth)
    - [How Auth Resolves](#how-auth-resolves)
    - [Credential Store](#credential-store)
    - [Environment Variables](#environment-variables)
- [Tools](#tools)
    - [Defining Tools](#defining-tools)
    - [Handling Tool Calls](#handling-tool-calls)
    - [Streaming Tool Calls with Partial JSON](#streaming-tool-calls-with-partial-json)
    - [Validating Tool Arguments](#validating-tool-arguments)
    - [Complete Event Reference](#complete-event-reference)
- [Image Input](#image-input)
- [Image Generation](#image-generation)
- [Thinking/Reasoning](#thinkingreasoning)
- [Stream Options](#stream-options)
- [Request Cancellation](#request-cancellation)
- [Stop Reasons](#stop-reasons)
- [Error Handling](#error-handling)
- [HTTP and WebSocket Proxies](#http-and-websocket-proxies)
- [Custom Providers](#custom-providers)
- [Faux Provider for Tests](#faux-provider-for-tests)
- [Cross-Provider Handoffs](#cross-provider-handoffs)
- [Context Serialization](#context-serialization)
- [OAuth Providers](#oauth-providers)
- [Development](#development)
- [License](#license)

## Supported Providers

- **OpenAI**
- **Ant Ling**
- **Azure OpenAI (Responses)**
- **OpenAI Codex** (ChatGPT Plus/Pro subscription, requires OAuth)
- **DeepSeek**
- **NVIDIA NIM**
- **Anthropic**
- **Google**
- **Vertex AI** (Gemini via Vertex AI)
- **Mistral**
- **Groq**
- **Cerebras**
- **Cloudflare AI Gateway**
- **Cloudflare Workers AI**
- **xAI**
- **OpenRouter**
- **Vercel AI Gateway**
- **ZAI Coding Plan (Global)** (with separate China provider)
- **MiniMax** (with separate China provider)
- **Together AI**
- **Hugging Face**
- **Moonshot AI** (with separate China provider)
- **GitHub Copilot** (requires OAuth)
- **Amazon Bedrock**
- **OpenCode Zen**
- **OpenCode Go**
- **Fireworks** (OpenAI- and Anthropic-compatible APIs)
- **Kimi For Coding** (Moonshot AI subscription endpoint, Anthropic-compatible API)
- **Xiaomi MiMo** (API billing endpoint, with separate Token Plan providers for `cn`/`ams`/`sgp`)
- **Any OpenAI-compatible API**: Ollama, vLLM, LM Studio, etc. (via custom providers)

Image generation is currently available through **OpenRouter** (`openrouter-images` API).

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
elph-ai = "0.0.21"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Or from the workspace:

```bash
cargo add elph-ai
```

## Quick Start

Build a `Models` collection of providers and stream through it. The quickest start registers every built-in provider; apps that only need a subset can register individual provider factories instead (see [Provider Factories](#provider-factories)).

```rust
use elph_ai::{
    AssistantContentBlock, AssistantMessageEvent, Context, Message, SimpleStreamOptions, Tool, UserContent,
    ThinkingLevel, builtin_models, get_builtin_model,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let models = builtin_models(None);
    let model = get_builtin_model("openai", "gpt-4o-mini")
        .ok_or_else(|| anyhow::anyhow!("model not found"))?;

    let tools = vec![Tool {
        name: "get_time".into(),
        description: "Get the current time".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "Optional timezone (e.g., America/New_York)"
                }
            }
        }),
    }];

    let context = Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User {
            content: UserContent::Text("What time is it?".into()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }],
        tools: Some(tools),
    };

    // Option 1: Streaming with all event types.
    // Auth resolves through the provider (OPENAI_API_KEY from the environment here).
    let stream = models.stream(&model, &context, None);
    let mut events = stream.into_stream();

    while let Some(event) = events.next().await {
        match event {
            AssistantMessageEvent::TextDelta { delta, .. } => print!("{delta}"),
            AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
                println!("\nTool called: {}({})", tool_call.name, tool_call.arguments);
            }
            AssistantMessageEvent::Done { reason, message } => {
                println!("\nFinished: {reason:?}");
                println!(
                    "Tokens: {} in, {} out, cost ${:.4}",
                    message.usage.input,
                    message.usage.output,
                    message.usage.cost.total
                );
            }
            AssistantMessageEvent::Error { error, .. } => {
                eprintln!("Error: {}", error.error_message.unwrap_or_default());
            }
            _ => {}
        }
    }

    // Option 2: Buffered completion without manually iterating events
    let response = models.complete(&model, &context, None).await;
    for block in &response.content {
        if let AssistantContentBlock::Text(text) = block {
            println!("{}", text.text);
        }
    }

    // Option 3: Simplified reasoning interface
    let _ = models
        .complete_simple(
            &model,
            &context,
            Some(SimpleStreamOptions {
                reasoning: Some(ThinkingLevel::Medium),
                ..Default::default()
            }),
        )
        .await;

    Ok(())
}
```

See [`examples/opencode_big_pickle.rs`](examples/opencode_big_pickle.rs) for a runnable example with progress output and streaming flags.

## Providers and Models

A **provider** is the runtime unit: it owns its model catalog, its auth (API key resolution, OAuth flows), and its stream behavior. A `Models` collection holds providers and routes every request to the provider that owns the model.

Providers internally share **API implementations** (the wire protocols): Anthropic models use `anthropic-messages`, OpenAI uses `openai-responses`, while xAI, Groq, Cerebras, OpenRouter, and most others share `openai-completions`. Mixed-API providers (GitHub Copilot, OpenCode Zen, Fireworks) dispatch per model.

Nine chat APIs are registered in `elph_ai::api::builtin_apis()`:

| API ID                    | Typical providers                                                       |
| ------------------------- | ----------------------------------------------------------------------- |
| `anthropic-messages`      | Anthropic, Kimi For Coding, parts of Fireworks / GitHub Copilot         |
| `openai-completions`      | Groq, Cerebras, OpenRouter, DeepSeek, Ollama-compatible endpoints, etc. |
| `openai-responses`        | OpenAI                                                                  |
| `openai-codex-responses`  | OpenAI Codex (ChatGPT subscription)                                     |
| `azure-openai-responses`  | Azure OpenAI                                                            |
| `google-generative-ai`    | Google Gemini                                                           |
| `google-vertex`           | Vertex AI                                                               |
| `mistral-conversations`   | Mistral                                                                 |
| `bedrock-converse-stream` | Amazon Bedrock                                                          |

Image generation uses a separate `openrouter-images` API (see [Image Generation](#image-generation)). Use `api_for("anthropic-messages")` or call modules under `elph_ai::api` for lower-level control.

Model catalogs are embedded as JSON under [`models/`](models/) and loaded at compile time via `include_str!`.

### Provider Factories

For apps that only need specific providers, register individual factories from `elph_ai::providers`:

```rust
use elph_ai::{create_models, MutableModels};
use elph_ai::providers::{anthropic_provider, openai_provider};

let mut models = create_models(None);
models.set_provider(anthropic_provider());
models.set_provider(openai_provider());
```

Additional factories live in `elph_ai::providers::builtin` (`amazon_bedrock_provider`, `google_vertex_provider`, `github_copilot_provider`, etc.). Use `builtin_providers()` to inspect the full list.

### All Built-in Providers

```rust
use elph_ai::builtin_models;

let models = builtin_models(None); // every built-in provider registered
```

`builtin_models()` accepts the same options as `create_models()` (`credentials`, `auth_context`). `builtin_providers()` returns the provider list if you want to register them on your own collection.

### Querying Models

Reads are synchronous and return the last-known lists:

```rust
let providers = models.get_providers();
let provider = models.get_provider("anthropic");

let all = models.get_models(None);
let anthropic_models = models.get_models(Some("anthropic"));
let model = models.get_model("anthropic", "claude-sonnet-4-5");

for m in anthropic_models {
    println!("{}: {}", m.id, m.name);
    println!("  API: {}", m.api);
    println!("  Context: {} tokens", m.context_window);
    println!("  Vision: {}", m.input.iter().any(|i| i == "image"));
    println!("  Reasoning: {}", m.reasoning);
}
```

Narrow dynamically looked-up models with `has_api()` when you need API-specific option typing:

```rust
use elph_ai::has_api;

if let Some(m) = models.get_model("anthropic", "claude-sonnet-4-5") {
    if has_api(&m, "anthropic-messages") {
        // stream options for anthropic-messages are available on StreamOptions
        let _ = models.stream(&m, &context, None);
    }
}
```

### Static Catalog Reads

For tooling that wants the embedded built-in catalog independent of any collection:

```rust
use elph_ai::{get_builtin_model, get_builtin_models, get_builtin_providers};

let model = get_builtin_model("openai", "gpt-4o-mini");
let providers = get_builtin_providers();
let anthropic = get_builtin_models("anthropic");
```

Image model catalogs are separate:

```rust
use elph_ai::images::get_builtin_image_models;

let openrouter_images = get_builtin_image_models("openrouter");
```

### Dynamic Providers

Providers may have dynamic model lists (a llama.cpp server, a live OpenRouter listing). Reads stay sync; fetching is an explicit async verb:

```rust
// get_models() returns the last-known list (empty before the first refresh)
models.refresh(Some("llamacpp")).await?; // one provider; rejects on failure
models.refresh(None).await?;            // all providers concurrently, best-effort
let fresh = models.get_model("llamacpp", "qwen3-30b");
```

Static built-in providers are no-ops for `refresh()`. See [Custom Providers](#custom-providers) for building a dynamic provider.

## Auth

Every provider owns its auth: how API keys resolve (stored credentials, environment variables, ambient sources like AWS profiles or gcloud ADC) and, where supported, OAuth login/refresh flows.

### How Auth Resolves

When you call `models.stream()`, the collection resolves auth through the owning provider and merges it into the request. Explicit per-request values always win:

```rust
// Resolved through the provider (env var, stored credential, OAuth token):
models.complete(&model, &context, None).await;

// Explicit key wins over anything the provider would resolve:
models.complete(
    &model,
    &context,
    Some(StreamOptions {
        api_key: Some("sk-explicit".into()),
        ..Default::default()
    }),
)
.await;
```

Inspect resolution without making a request — useful for status UIs:

```rust
let auth = models.get_auth(&model).await?;
if let Some(auth) = &auth {
    println!("configured via {}", auth.source.as_deref().unwrap_or("unknown"));
} else {
    println!("not configured");
}
```

`get_auth()` returns `Ok(None)` for unconfigured providers and `Err(ModelsError)` when something is broken (`ModelsErrorCode::Oauth`: token refresh failed; `ModelsErrorCode::Auth`: key resolution or credential store failure). Request paths surface the same failures as stream errors.

### Credential Store

Stored credentials (API keys entered interactively, OAuth tokens) live in a `CredentialStore` — one type-tagged credential per provider. elph-ai ships an in-memory default; apps inject persistent storage:

```rust
use elph_ai::{CreateModelsOptions, create_models, InMemoryCredentialStore};
use std::sync::Arc;

let models = create_models(Some(CreateModelsOptions {
    credentials: Some(Arc::new(my_file_backed_store)),
    auth_context: None,
}));
```

The contract is small: `read(provider_id)`, `modify(provider_id, fn)` (serialized read-modify-write), and `delete(provider_id)`. OAuth token refresh runs inside `modify`, so concurrent requests cannot double-refresh a rotated token. A stored credential _owns_ its provider: environment variables are only consulted when nothing is stored.

API-key credentials can carry provider-scoped env/config values:

```json
{
    "type": "api_key",
    "key": "...",
    "env": {
        "CLOUDFLARE_ACCOUNT_ID": "account-id",
        "CLOUDFLARE_GATEWAY_ID": "gateway-id"
    }
}
```

### Environment Variables

Built-in providers resolve these environment variables:

| Provider                               | Environment Variable(s)                                                                                                                                              |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OpenAI                                 | `OPENAI_API_KEY`                                                                                                                                                     |
| Ant Ling                               | `ANT_LING_API_KEY`                                                                                                                                                   |
| Azure OpenAI                           | `AZURE_OPENAI_API_KEY` + `AZURE_OPENAI_BASE_URL` or `AZURE_OPENAI_RESOURCE_NAME`. Optional: `AZURE_OPENAI_API_VERSION`, `AZURE_OPENAI_DEPLOYMENT_NAME_MAP`           |
| Anthropic                              | `ANTHROPIC_API_KEY` or `ANTHROPIC_OAUTH_TOKEN`                                                                                                                       |
| DeepSeek                               | `DEEPSEEK_API_KEY`                                                                                                                                                   |
| NVIDIA NIM                             | `NVIDIA_API_KEY`                                                                                                                                                     |
| Google                                 | `GEMINI_API_KEY`                                                                                                                                                     |
| Vertex AI                              | `GOOGLE_CLOUD_API_KEY` or `GOOGLE_CLOUD_PROJECT` (or `GCLOUD_PROJECT`) + `GOOGLE_CLOUD_LOCATION` + ADC                                                               |
| Mistral                                | `MISTRAL_API_KEY`                                                                                                                                                    |
| Groq                                   | `GROQ_API_KEY`                                                                                                                                                       |
| Cerebras                               | `CEREBRAS_API_KEY`                                                                                                                                                   |
| Cloudflare AI Gateway                  | `CLOUDFLARE_API_KEY` + `CLOUDFLARE_ACCOUNT_ID` + `CLOUDFLARE_GATEWAY_ID`                                                                                             |
| Cloudflare Workers AI                  | `CLOUDFLARE_API_KEY` + `CLOUDFLARE_ACCOUNT_ID`                                                                                                                       |
| xAI                                    | `XAI_API_KEY`                                                                                                                                                        |
| Fireworks                              | `FIREWORKS_API_KEY`                                                                                                                                                  |
| Together AI                            | `TOGETHER_API_KEY`                                                                                                                                                   |
| OpenRouter                             | `OPENROUTER_API_KEY`                                                                                                                                                 |
| Vercel AI Gateway                      | `VERCEL_AI_GATEWAY_API_KEY`                                                                                                                                          |
| ZAI Coding Plan (Global)               | `ZAI_API_KEY`                                                                                                                                                        |
| ZAI Coding Plan (China)                | `ZAI_CODING_CN_API_KEY`                                                                                                                                              |
| MiniMax (Global)                       | `MINIMAX_API_KEY`                                                                                                                                                    |
| MiniMax (China)                        | `MINIMAX_CN_API_KEY`                                                                                                                                                 |
| Moonshot AI / Moonshot AI (China)      | `MOONSHOT_API_KEY`                                                                                                                                                   |
| Hugging Face                           | `HF_TOKEN`                                                                                                                                                           |
| OpenCode Zen / OpenCode Go             | `OPENCODE_API_KEY`                                                                                                                                                   |
| Kimi For Coding                        | `KIMI_API_KEY`                                                                                                                                                       |
| Xiaomi MiMo (API billing)              | `XIAOMI_API_KEY`                                                                                                                                                     |
| Xiaomi MiMo Token Plan (China/AMS/SGP) | `XIAOMI_API_KEY`                                                                                                                                                     |
| GitHub Copilot                         | `COPILOT_GITHUB_TOKEN`                                                                                                                                               |
| Amazon Bedrock                         | `AWS_REGION` or `AWS_DEFAULT_REGION`, `AWS_PROFILE`, `AWS_BEARER_TOKEN_BEDROCK` (bearer auth path). Optional: `AWS_BEDROCK_FORCE_CACHE=1`, `PI_CACHE_RETENTION=long` |

Amazon Bedrock also resolves ambient AWS credentials (access key pairs, ECS task roles, web identity tokens) when no bearer token is set. Vertex AI resolves either an explicit key or gcloud Application Default Credentials plus project/location.

Per-request `StreamOptions.env` and stored credential `env` maps override process environment for the same keys. Global tuning variables:

| Variable                                   | Effect                                                                                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `PI_CACHE_RETENTION=long`                  | Default prompt cache retention to `long` when `cache_retention` is unset (Anthropic, OpenAI Responses, Bedrock Claude models) |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` | HTTP(S) proxy for outbound provider requests (see [HTTP and WebSocket Proxies](#http-and-websocket-proxies))                  |
| `NO_PROXY`                                 | Hostnames/ports to bypass the proxy (`*`, comma-separated, optional `:port` suffix)                                           |

## Tools

Tools enable LLMs to interact with external systems. Tool parameters are [JSON Schema](https://json-schema.org/) values (`serde_json::Value`), validated with the `jsonschema` crate.

### Defining Tools

```rust
use elph_ai::Tool;
use serde_json::json;

let weather_tool = Tool {
    name: "get_weather".into(),
    description: "Get current weather for a location".into(),
    parameters: json!({
        "type": "object",
        "properties": {
            "location": { "type": "string", "description": "City name or coordinates" },
            "units": { "type": "string", "enum": ["celsius", "fahrenheit"], "default": "celsius" }
        },
        "required": ["location"]
    }),
};
```

For Google API compatibility, prefer `enum` arrays over complex `anyOf`/`const` patterns in schemas.

### Handling Tool Calls

Tool results use content blocks and can include both text and images:

```rust
use elph_ai::{ContentBlock, Message};

let response = models.complete(&model, &context, None).await;

for block in &response.content {
    if let AssistantContentBlock::ToolCall(call) = block {
        context.messages.push(Message::ToolResult {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            content: vec![ContentBlock::Text {
                text: r#"{"temp": 18}"#.into(),
            }],
            details: None,
            is_error: false,
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
    }
}

// Tool results can also include images (for vision-capable models)
context.messages.push(Message::ToolResult {
    tool_call_id: "tool_xyz".into(),
    tool_name: "generate_chart".into(),
    content: vec![
        ContentBlock::Text {
            text: "Generated chart showing temperature trends".into(),
        },
        ContentBlock::Image {
            data: base64_chart,
            mime_type: "image/png".into(),
        },
    ],
    details: None,
    is_error: false,
    timestamp: chrono::Utc::now().timestamp_millis(),
});
```

### Streaming Tool Calls with Partial JSON

During streaming, tool call arguments are progressively parsed as they arrive:

```rust
let stream = models.stream(&model, &context, None);
let mut events = stream.into_stream();

while let Some(event) = events.next().await {
    match event {
        AssistantMessageEvent::ToolcallDelta { partial, content_index, .. } => {
            if let Some(AssistantContentBlock::ToolCall(call)) = partial.content.get(content_index) {
                // BE DEFENSIVE: arguments may be incomplete during streaming
                if call.name == "write_file" {
                    if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                        println!("Writing to: {path}");
                    }
                }
            }
        }
        AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
            println!("Tool completed: {} {:?}", tool_call.name, tool_call.arguments);
        }
        _ => {}
    }
}
```

**Important notes about partial tool arguments:**

- During `ToolcallDelta` events, `arguments` contains the best-effort parse of partial JSON
- Fields may be missing or incomplete — always check for existence before use
- At minimum, `arguments` will be an empty object, never missing
- The Google provider does not support function call streaming; you receive a single `ToolcallDelta` with full arguments

### Validating Tool Arguments

Use `validate_tool_call` before executing tools:

```rust
use elph_ai::validation::validate_tool_call;

if let AssistantMessageEvent::ToolcallEnd { tool_call, .. } = event {
    let tool = tools.iter().find(|t| t.name == tool_call.name).unwrap();
    match validate_tool_call(tool, &tool_call) {
        Ok(()) => { /* execute tool */ }
        Err(msg) => {
            context.messages.push(Message::ToolResult {
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                content: vec![ContentBlock::Text { text: msg }],
                details: None,
                is_error: true,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
        }
    }
}
```

### Complete Event Reference

| Event           | Description              | Key fields                                          |
| --------------- | ------------------------ | --------------------------------------------------- |
| `Start`         | Stream begins            | `partial`                                           |
| `TextStart`     | Text block starts        | `content_index`                                     |
| `TextDelta`     | Text chunk received      | `delta`, `content_index`                            |
| `TextEnd`       | Text block complete      | `content`, `content_index`                          |
| `ThinkingStart` | Thinking block starts    | `content_index`                                     |
| `ThinkingDelta` | Thinking chunk received  | `delta`, `content_index`                            |
| `ThinkingEnd`   | Thinking block complete  | `content`, `content_index`                          |
| `ToolcallStart` | Tool call begins         | `content_index`                                     |
| `ToolcallDelta` | Tool arguments streaming | `delta`, `partial.content[content_index].arguments` |
| `ToolcallEnd`   | Tool call complete       | `tool_call`                                         |
| `Done`          | Stream complete          | `reason`, `message`                                 |
| `Error`         | Stream failed            | `reason`, `error`                                   |

Events serialize as snake_case JSON (`text_delta`, `toolcall_end`, etc.) via `serde`.

## Image Input

User messages support text and image content blocks for vision-capable models:

```rust
let context = Context {
    messages: vec![Message::User {
        content: UserContent::Blocks(vec![
            ContentBlock::Text { text: "What's in this image?".into() },
            ContentBlock::Image {
                data: base64_png,
                mime_type: "image/png".into(),
            },
        ]),
        timestamp: chrono::Utc::now().timestamp_millis(),
    }],
    system_prompt: None,
    tools: None,
};
```

## Image Generation

Image generation uses a separate API surface from text/chat generation: an `ImagesModels` collection holds `ImagesProvider`s, reads are sync, and auth resolves through the owning provider. Image generation is one-shot — use `generate_images()`, not the chat/stream APIs.

```rust
use elph_ai::{builtin_images_models, ContentBlock, ImagesContext};
use elph_ai::images::get_builtin_image_models;

let images = builtin_images_models(None);
let model = get_builtin_image_models("openrouter")
    .into_iter()
    .find(|m| m.id.contains("gemini"))
    .unwrap();

let result = images
    .generate_images(
        &model,
        &ImagesContext {
            input: vec![ContentBlock::Text {
                text: "Generate a red circle on a plain white background.".into(),
            }],
        },
        None,
    )
    .await;

for block in &result.output {
    match block {
        ContentBlock::Text { text } => println!("{text}"),
        ContentBlock::Image { mime_type, .. } => println!("image: {mime_type}"),
    }
}
```

Check capabilities on model metadata:

```rust
println!("input: {:?}", model.input);   // ["text", "image"]
println!("output: {:?}", model.output); // ["image"] or ["image", "text"]
```

Failures return an `AssistantImages` with `stop_reason: Error` rather than panicking.

`ImagesOptions` mirrors chat `StreamOptions` for auth, timeouts, retries, proxy env, payload/response hooks, and cancellation:

```rust
use elph_ai::ImagesOptions;
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
// cancel token when the user dismisses the UI
images.generate_images(
    &model,
    &context,
    Some(ImagesOptions {
        signal: Some(token),
        api_key: None,
        env: None,
        headers: None,
        timeout_ms: None,
        max_retries: None,
        on_payload: None,
        on_response: None,
    }),
)
.await;
```

## Thinking/Reasoning

Many models support thinking/reasoning. Check `model.reasoning`; options passed to non-reasoning models are silently ignored.

### Unified Interface (`stream_simple` / `complete_simple`)

```rust
use elph_ai::{SimpleStreamOptions, ThinkingLevel};

let response = models
    .complete_simple(
        &model,
        &context,
        Some(SimpleStreamOptions {
            reasoning: Some(ThinkingLevel::Medium),
            ..Default::default()
        }),
    )
    .await;

for block in &response.content {
    match block {
        AssistantContentBlock::Thinking(t) => println!("Thinking: {}", t.thinking),
        AssistantContentBlock::Text(t) => println!("Response: {}", t.text),
        _ => {}
    }
}
```

Use `get_supported_thinking_levels()` and `clamp_thinking_level()` to respect per-model capability maps.

### Provider-Specific Options (`stream` / `complete`)

`stream()` / `complete()` accept the owning API's full `StreamOptions`. Use `has_api()` to narrow models before passing API-specific fields (`thinking_enabled`, `reasoning_effort`, etc.).

### Streaming Thinking Content

```rust
while let Some(event) = events.next().await {
    match event {
        AssistantMessageEvent::ThinkingDelta { delta, .. } => print!("{delta}"),
        AssistantMessageEvent::ThinkingEnd { .. } => println!("\n[Thinking complete]"),
        _ => {}
    }
}
```

## Stream Options

`stream()` / `complete()` accept `StreamOptions`; `stream_simple()` / `complete_simple()` wrap the same fields in `SimpleStreamOptions.base` plus reasoning knobs.

| Field                                        | Purpose                                                                                                              |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `temperature`, `max_tokens`                  | Sampling controls (`max_tokens` is clamped to remaining context)                                                     |
| `api_key`                                    | Explicit key; overrides provider auth resolution                                                                     |
| `transport`                                  | Transport hint (`sse`, `websocket`, `websocket-cached`, `auto`) — used by OpenAI Codex when calling the API directly |
| `cache_retention`                            | Prompt cache retention (`none`, `short`, `long`); defaults from `PI_CACHE_RETENTION`                                 |
| `session_id`                                 | Stable session id (Codex WebSocket context reuse, request tracing headers)                                           |
| `headers`                                    | Per-request header overrides (`None` removes a model default header)                                                 |
| `timeout_ms`, `websocket_connect_timeout_ms` | HTTP and Codex WebSocket connect timeouts                                                                            |
| `max_retries`, `max_retry_delay_ms`          | Retry policy for transient failures                                                                                  |
| `metadata`                                   | Opaque JSON attached to provider payloads where supported                                                            |
| `env`                                        | Scoped environment map (proxy vars, Bedrock region, provider config)                                                 |
| `on_payload`                                 | Async hook to inspect or rewrite the outgoing JSON body                                                              |
| `on_response`                                | Async hook invoked with HTTP status/headers after the provider responds                                              |
| `signal`                                     | `CancellationToken` for cooperative abort (see [Request Cancellation](#request-cancellation))                        |

Provider-specific option structs (`AnthropicOptions`, `OpenAICompletionsOptions`, `BedrockOptions`, etc.) extend `base: StreamOptions` when you call `elph_ai::api` modules directly.

Chat streams parse provider SSE incrementally — events are emitted as chunks arrive rather than buffering the full response body first.

## Request Cancellation

Pass a `tokio_util::sync::CancellationToken` in `StreamOptions.signal` (or `SimpleStreamOptions.base.signal` / `ImagesOptions.signal`). When cancelled:

- In-flight HTTP requests and SSE parsers stop promptly
- The final `AssistantMessage` / `AssistantImages` uses `stop_reason: Aborted`
- Mid-stream partial content may be preserved on the final message when cancellation happens during generation

```rust
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
let stream = models.stream(
    &model,
    &context,
    Some(StreamOptions {
        signal: Some(token.clone()),
        ..Default::default()
    }),
);

// elsewhere: token.cancel();
let message = stream.result().await;
assert_eq!(message.stop_reason, StopReason::Aborted);
```

Cancellation is checked before the request is sent, while waiting on the network, and between SSE/WebSocket events. See [`tests/abort.rs`](tests/abort.rs) and [`tests/sse_abort.rs`](tests/sse_abort.rs).

## Stop Reasons

| Reason    | Meaning                   |
| --------- | ------------------------- |
| `Stop`    | Natural completion        |
| `Length`  | Hit `max_tokens`          |
| `ToolUse` | Model wants to call tools |
| `Error`   | Request failed            |
| `Aborted` | Request was cancelled     |

## Error Handling

Request failures do not panic out of stream functions. Errors arrive as `AssistantMessageEvent::Error` and the final message carries details:

```rust
while let Some(event) = events.next().await {
    if let AssistantMessageEvent::Error { error, reason } = event {
        eprintln!("Error ({reason:?}): {}", error.error_message.unwrap_or_default());
    }
}

let message = stream.result().await;
if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted) {
    eprintln!("Request failed: {}", message.error_message.unwrap_or_default());
    // message.content may contain partial content received before the error
}
```

### Debugging Provider Payloads

Use the `on_payload` callback in `StreamOptions` to inspect the request payload sent to the provider:

```rust
use std::{future::Future, pin::Pin, sync::Arc};

let options = StreamOptions {
    on_payload: Some(Arc::new(|payload, _model| {
        Box::pin(async move {
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
            Some(payload)
        }) as Pin<Box<dyn Future<Output = Option<serde_json::Value>> + Send>>
    })),
    ..Default::default()
};
```

Supported by `stream`, `complete`, `stream_simple`, and `complete_simple`.

Use `on_response` to capture raw HTTP metadata (status, headers) without logging full bodies:

```rust
use elph_ai::ProviderResponse;
use elph_ai::api::common::wrap_on_response;

let options = StreamOptions {
    on_response: Some(wrap_on_response(|response: ProviderResponse, _model| {
        Box::pin(async move {
            println!("status {}", response.status);
        })
    })),
    ..Default::default()
};
```

## HTTP and WebSocket Proxies

Outbound HTTP(S) provider traffic respects standard proxy environment variables, including values supplied through `StreamOptions.env` / stored credential `env` maps:

- `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` — must be `http://` or `https://` URLs (SOCKS and PAC are not supported)
- `NO_PROXY` — comma- or whitespace-separated hostnames; prefix with `.` or `*` for suffix/wildcard matches; optional `:port`

WebSocket URLs (`ws://`, `wss://`) map to `http://` / `https://` for proxy rule lookup. OpenAI Codex WebSocket transport tunnels through HTTPS proxies (CONNECT + nested TLS). See [`tests/http_proxy.rs`](tests/http_proxy.rs) and [`tests/codex_websocket_proxy.rs`](tests/codex_websocket_proxy.rs).

## Custom Providers

Build providers with `create_provider()`:

```rust
use elph_ai::{CreateProviderOptions, ProviderApi, ProviderAuth, create_provider, env_api_key_auth};
use elph_ai::providers::adapter::openai_completions_api;

let provider = create_provider(CreateProviderOptions {
    id: "my-llm".into(),
    name: Some("My LLM".into()),
    base_url: Some("http://localhost:11434/v1".into()),
    headers: None,
    auth: ProviderAuth {
        api_key: Some(env_api_key_auth("API key", vec!["MY_LLM_API_KEY"])),
        oauth: None,
    },
    models: my_models,
    refresh_models: None, // or Some(Arc::new(|| Box::pin(async { ... })))
    api: ProviderApi::Single(openai_completions_api()),
});
```

Call API implementations directly from `elph_ai::api` for lower-level control, or register custom providers on a `Models` collection.

## Faux Provider for Tests

`faux_provider()` builds an in-memory provider with scripted responses:

```rust
use elph_ai::{
    create_models, faux_assistant_message, faux_provider, faux_text, faux_thinking, faux_tool_call,
    FauxResponseStep, StopReason,
};
use serde_json::json;

let faux = faux_provider(Default::default());
let mut models = create_models(None);
models.set_provider(faux.provider.clone());

let model = faux.provider.get_models()[0].clone();
faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
    vec![
        faux_thinking("Need to inspect package metadata first."),
        faux_tool_call("echo", json!({ "text": "package.json" }), None),
    ],
    Some(StopReason::ToolUse),
))]);
```

Notes:

- Responses are consumed from a queue in request order
- An empty queue returns an assistant error: `"No more faux responses queued"`
- Use `set_responses()` to replace the queue and `append_responses()` to extend it
- Tool call arguments stream incrementally via `ToolcallDelta` events

See [`tests/faux_provider.rs`](tests/faux_provider.rs) for integration coverage.

## Cross-Provider Handoffs

The library supports seamless handoffs between providers within the same conversation. When messages from one provider are sent to another, `transform_messages` adapts them for compatibility:

- **User and tool result messages** pass through unchanged
- **Assistant messages from the same provider/API** are preserved as-is
- **Assistant messages from different providers** have thinking blocks converted to `<thinking>` tagged text
- **Tool calls and regular text** are preserved unchanged

```rust
use elph_ai::providers::{anthropic_provider, openai_provider};

let mut models = create_models(None);
models.set_provider(anthropic_provider());
models.set_provider(openai_provider());
// register additional providers as needed

let mut context = Context { messages: vec![], system_prompt: None, tools: None };

let claude = models.get_model("anthropic", "claude-sonnet-4-5").unwrap();
context.messages.push(Message::User { /* ... */ });
context.messages.push(models.complete_simple(&claude, &context, None).await);

let gpt = models.get_model("openai", "gpt-5-mini").unwrap();
context.messages.push(Message::User { /* ... */ });
context.messages.push(models.complete(&gpt, &context, None).await);
```

See [`tests/transform_messages.rs`](tests/transform_messages.rs).

## Context Serialization

`Context`, `Message`, and `AssistantMessage` derive `Serialize`/`Deserialize`. Contexts are plain JSON-friendly structs you can persist, send over the wire, and resume with a different model:

```rust
let json = serde_json::to_string(&context)?;
let restored: Context = serde_json::from_str(&json)?;
let response = models.complete(&other_model, &restored, None).await;
```

## OAuth Providers

Built-in OAuth flows are available for Anthropic, GitHub Copilot, and OpenAI Codex via `elph_ai::auth::oauth`:

```rust
use elph_ai::auth::oauth::{login_anthropic, register_oauth_provider, anthropic_oauth};
use std::sync::Arc;

register_oauth_provider(anthropic_oauth());
let tokens = login_anthropic(&callbacks).await?;
```

Use `get_oauth_api_key()`, `refresh_oauth_token()`, and the `CredentialStore` to persist tokens across sessions.

### OpenAI Codex Transport

OpenAI Codex models (`openai-codex-responses` API) support SSE and WebSocket transports with automatic fallback when connection limits are hit. Collection-level `models.stream()` defaults to `auto` (WebSocket with cached context when `session_id` is set, SSE fallback on limit errors).

For explicit transport control, call the API module directly:

```rust
use elph_ai::api::codex_transport::CodexTransport;
use elph_ai::api::{OpenAICodexResponsesApi, OpenAICodexResponsesOptions};

let api = OpenAICodexResponsesApi;
let _stream = api.stream_with_options(
    &model,
    &context,
    OpenAICodexResponsesOptions {
        transport: CodexTransport::WebSocketCached,
        base: StreamOptions {
            session_id: Some("my-session".into()),
            env: Some(proxy_env), // HTTP_PROXY / NO_PROXY for corporate networks
            ..Default::default()
        },
        ..Default::default()
    },
);
```

`CodexTransport` values: `Auto`, `Sse`, `WebSocket`, `WebSocketCached`. Debug helpers (`get_codex_websocket_debug_stats`, `close_codex_websocket_sessions`) are exported from the crate root.

### Vertex AI

Vertex AI supports either an API key or Application Default Credentials:

```bash
# Local ADC
gcloud auth application-default login
export GOOGLE_CLOUD_PROJECT="my-project"
export GOOGLE_CLOUD_LOCATION="us-central1"

# CI/Production
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```

## Development

### Regenerating Model Catalogs

Chat and image model catalogs are generated from [pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai) scripts:

```bash
# From the repo root (requires upstream catalog checkout and npm deps)
make generate-models ELPH_AI_CATALOG_DIR=/path/to/catalog/packages/ai

# Or directly:
cargo run -p elph-ai --bin generate-models -- all --catalog-dir /path/to/catalog/packages/ai

# Convert existing catalog output without re-running npm scripts:
make generate-models ELPH_AI_CATALOG_DIR=/path/to/catalog/packages/ai ARGS="--skip-scripts"
```

Subcommands:

| Command      | Catalog npm script      | Output                                          |
| ------------ | ----------------------- | ----------------------------------------------- |
| `chat`       | `generate-models`       | `models/*.json` + `src/models/catalog.rs`       |
| `image`      | `generate-image-models` | `models/images/*.json` + `src/images/models.rs` |
| `test-image` | `generate-test-image`   | `tests/data/red-circle.png`                     |
| `all`        | all of the above           | everything                                      |

### Adding a New Provider

1. **Types** (`src/types/mod.rs`) — add API/provider identifiers and options if needed
2. **API** (`src/api/<api-id>.rs`) — implement `stream` / `stream_simple` for new wire protocols
3. **Catalog** — add fetch logic to the upstream catalog `generate-models` script, then run `make generate-models`
4. **Provider factory** (`src/providers/builtin.rs`) — wire catalog + auth + API adapter; register in `builtin_providers()`
5. **Tests** (`tests/`) — streaming, tools, auth, cross-provider handoff as applicable

### Running Tests

```bash
# Unit and integration tests (default — skips #[ignore] live tests)
cargo nextest run -p elph-ai

# Run all tests including live provider tests (requires API keys)
cargo nextest run -p elph-ai -- --ignored

# Individual live test binaries
cargo nextest run -p elph-ai --test e2e_live -- --ignored
cargo nextest run -p elph-ai --test abort_live -- --ignored
cargo nextest run -p elph-ai --test cross_provider_handoff_live -- --ignored
cargo nextest run -p elph-ai --test openrouter_cache_write_live -- --ignored
cargo nextest run -p elph-ai --test tool_call_id_normalization_live -- --ignored
```

Integration tests mirror upstream coverage: provider auth, SSE parsing and mid-stream abort, HTTP/WebSocket proxy routing, tool schemas, retry/overflow, OAuth, Bedrock endpoint resolution, Codex WebSocket transport, faux provider, and more under [`tests/`](tests/).

## License

Licensed under the [MIT License](https://www.tldrlegal.com/license/mit-license).
