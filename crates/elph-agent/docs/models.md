# Models integration

This document describes how `elph-agent` integrates with `elph_ai::Models`. For the full provider architecture, auth, and API implementation details, see the [`elph-ai`](../../elph-ai) crate.

## Overview

`AgentHarness` holds an `Arc<Models>` and delegates provider streaming to it. The low-level `Agent` class uses `builtin_models()` by default and wraps `Models::stream_simple` as the default `stream_fn`.

```
AgentHarness
  └─ models: Arc<Models>
       └─ get_provider(model.provider)
            └─ provider.stream_simple(model, context, options)
```

The harness does not own model listing, auth resolution, or provider registration — that is `elph-ai`'s responsibility.

## Creating a Models instance

### Built-in providers (all providers)

```rust
use elph_ai::builtin_models;

let models = builtin_models(None);
let model = models
    .get_model("anthropic", "claude-sonnet-4-20250514")
    .expect("model not found");
```

### Custom provider set

```rust
use elph_ai::{create_models, providers::openai_provider};

let mut models = create_models(None);
models.set_provider(openai_provider());
```

### Test provider (faux)

```rust
use elph_ai::{builtin_models, faux_assistant_message, faux_provider, faux_text, Models, StopReason};

let faux = faux_provider(Default::default());
faux.set_responses(vec![faux_assistant_message(faux_text("Hi!"), StopReason::Stop)]);

let mut models = builtin_models(None);
models.set_provider(faux.provider.clone());
let models: Arc<Models> = models.into_arc();
```

## AgentHarness model configuration

`AgentHarnessOptions` requires both a `models` registry and an initial `model`:

```rust
use elph_agent::{AgentHarness, AgentHarnessOptions};
use elph_ai::{Models, builtin_models};
use std::sync::Arc;

let models: Arc<Models> = builtin_models(None).into_arc();
let model = models.get_model("openai", "gpt-4o").unwrap();

let harness = AgentHarness::new(AgentHarnessOptions {
    models: models.clone(),
    model,
    // ...
})?;
```

### Changing model at runtime

```rust
let new_model = harness.models().get_model("anthropic", "claude-sonnet-4-20250514").unwrap();
harness.set_model(new_model).await?;
```

`set_model` updates harness config immediately. If a turn is in flight, the change applies at the next save point, not to the current provider request.

Model changes can be persisted as session entries (branch-scoped `model_change`).

## Agent model configuration

The `Agent` class takes a concrete `Model` in `PartialAgentState`:

```rust
use elph_agent::{Agent, AgentOptions, PartialAgentState};
use elph_ai::builtin_models;

let models = builtin_models(None);
let model = models.get_model("openai", "gpt-4o").unwrap();

let agent = Agent::new(AgentOptions {
    initial_state: Some(PartialAgentState {
        model: Some(model),
        ..Default::default()
    }),
    ..Default::default()
});
```

Change model at runtime via state:

```rust
let mut state = agent.state().await;
state.model = new_model;
// Agent state is read on each run; update via MutableAgentState accessors
```

## Stream options

### Agent

`Agent` builds `SimpleStreamOptions` from `thinking_level`, `session_id`, `transport`, `thinking_budgets`, and `max_retry_delay_ms` in `AgentLoopConfig`.

### AgentHarness

`AgentHarnessOptions.stream_options` is snapshotted per turn. Use `get_stream_options()` / `set_stream_options()` for runtime changes.

Provider hooks can patch stream options per request:

- `before_provider_request` — patch `AgentHarnessStreamOptions`
- `before_provider_payload` — transform the serialized provider payload

Auth is resolved per provider request via `get_api_key` in stream options, so expiring OAuth tokens can refresh without restarting the turn.

## Thinking level

`AgentThinkingLevel` includes harness-only `Off` in addition to the standard levels (`Minimal`, `Low`, `Medium`, `High`, `Xhigh`).

At the provider boundary:

- `Off` → no reasoning / `None`
- other levels → `ThinkingLevel` for `SimpleStreamOptions.reasoning`

```rust
use elph_agent::AgentThinkingLevel;

harness.set_thinking_level(AgentThinkingLevel::Medium).await?;
```

## Auth

`elph-ai` owns credential storage and `Models::get_auth()`. The harness passes resolved credentials into stream options at request time.

For dynamic API key resolution in `Agent`:

```rust
use elph_agent::AgentOptions;

let agent = Agent::new(AgentOptions {
    get_api_key: Some(Arc::new(|provider| {
        Box::pin(async move { refresh_token_for(provider).await })
    })),
    ..Default::default()
});
```

## Proxy streaming

When the client cannot call providers directly, use `stream_proxy` as the `stream_fn`:

```rust
use elph_agent::{ProxyStreamOptions, stream_proxy};

let stream_fn = Arc::new(|model, context, options| {
    stream_proxy(
        model,
        context,
        ProxyStreamOptions {
            base: options.unwrap_or_default(),
            auth_token: token,
            proxy_url: proxy_url,
        },
    )
});
```

## Model refresh

Dynamic providers (OpenRouter, llama.cpp, etc.) expose `refresh_models()`. Call `models.refresh(Some("provider_id")).await` before `get_model()` when freshness matters:

```rust
models.refresh(Some("openrouter")).await?;
let model = models.get_model("openrouter", "anthropic/claude-sonnet-4").unwrap();
```

Static providers (built-in catalogs) return their full catalog from `get_models()` without refresh.

## Relationship to pi-ai Models refactor

The upstream [pi-ai Models architecture](https://github.com/earendil-works/pi/blob/main/packages/agent/docs/models.md) describes a target design with:

- `create_models()` as a dumb provider collection
- per-provider factories under `providers/`
- lazy API implementations under `api/`
- explicit `refresh()` for dynamic model lists
- injected `CredentialStore` and fixed auth resolution policy

`elph-ai` follows this architecture. `elph-agent` consumes `Models` as a dependency and does not reimplement provider dispatch.

## What elph-agent does not do

- Provider registration (use `elph-ai`)
- Model catalog generation (use `elph-ai` provider modules)
- OAuth login flows (use `elph-ai` auth)
- Per-harness model registry validation (planned — see [agent-harness.md](./agent-harness.md))
