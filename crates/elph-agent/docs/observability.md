# Observability design notes

**Status: planned, not yet implemented** in `elph-agent` or `elph-ai`.

Design adapted from [pi-agent observability](https://github.com/earendil-works/pi/blob/main/packages/agent/docs/observability.md) for the Rust/Elph stack.

## Goal

Make `elph-ai` and `elph-agent`/harness observable without depending on OpenTelemetry, Sentry, or any APM vendor.

Elph should emit stable, structured lifecycle events. External listeners can convert those events into OTel spans, Sentry spans, logs, metrics, or custom telemetry.

## Mental model

A trace is one causal tree of work, e.g. one user turn.

A span is one timed operation in that tree:

```rust
struct SpanRecord {
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    name: String,
    start_time: u64,
    end_time: Option<u64>,
    attributes: HashMap<String, Value>,
    status: SpanStatus, // Ok | Error
}
```

Example tree:

```text
trace_id=t1 span_id=s1 parent=-  name=elph.agent.prompt
trace_id=t1 span_id=s2 parent=s1 name=elph.agent.turn
trace_id=t1 span_id=s3 parent=s2 name=elph.ai.provider.request
trace_id=t1 span_id=s4 parent=s2 name=elph.agent.tool_call
trace_id=t1 span_id=s5 parent=s4 name=elph.session.append_entry
```

## Async context

Rust has multiple async tasks that can interleave on the same runtime. A single global context breaks under concurrency.

The Rust equivalent of Node's `AsyncLocalStorage` is `tokio::task_local!` or a tracing subscriber layer with span extensions. For cross-crate use, a small runtime-agnostic context type is preferable:

```rust
// Planned
pub struct ElphObservabilityContext {
    pub trace_id: Option<String>,
    pub current_span_id: Option<String>,
    pub user_context: Option<HashMap<String, Value>>,
}
```

Deep code can read the correct current context for the active async task.

Core packages should not depend on a specific tracing backend. Adapters bridge to `tracing`, OpenTelemetry, etc.

## Core design

Planned runtime-agnostic abstraction:

```rust
pub enum ObservabilityEventType {
    Start,
    End,
    Error,
    Event,
}

pub struct ElphObservabilityEvent {
    pub event_type: ObservabilityEventType,
    pub name: String,
    pub trace_id: String,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub timestamp: u64,
    pub duration_ms: Option<u64>,
    pub context: Option<HashMap<String, Value>>,
    pub payload: Option<HashMap<String, Value>>,
    pub error: Option<ObservabilityError>,
}

pub trait ElphObservability: Send + Sync {
    fn get_context(&self) -> Option<ElphObservabilityContext>;
    fn run_with_context<T>(&self, context: ElphObservabilityContext, f: impl FnOnce() -> T) -> T;
    fn emit(&self, event: ElphObservabilityEvent);
    fn has_subscribers(&self) -> bool;
}
```

Public API (planned):

```rust
pub fn configure_elph_observability(obs: Arc<dyn ElphObservability>);
pub fn subscribe_elph_observability(listener: ObservabilityListener) -> impl FnOnce();
pub fn run_with_elph_context<T>(user_context: HashMap<String, Value>, f: impl FnOnce() -> T) -> T;
pub async fn trace_operation<T>(name: &str, payload: HashMap<String, Value>, f: impl Future<Output = T>) -> T;
```

`trace_operation()`:

1. reads the current context
2. creates `trace_id` if missing
3. creates a new `span_id`
4. uses current span as `parent_span_id`
5. emits `Start`
6. runs callback under child context
7. emits `End` or `Error`
8. rethrows on error

## What elph emits

Elph emits what happened. It does not create OTel/Sentry spans directly.

Initial minimal event names:

```text
elph.agent.prompt
elph.agent.skill
elph.agent.prompt_template
elph.agent.compaction
elph.agent.branch_navigation
elph.agent.session.append_entry
elph.ai.provider.request
```

Each operation emits `start`, `end`, and `error`.

Later additions:

```text
elph.agent.turn
elph.agent.tool_call
elph.agent.queue_update
elph.ai.provider.retry
elph.ai.provider.first_token
elph.ai.provider.usage
elph.session.read
elph.session.write
```

## Minimal instrumentation points

### elph-agent

Wrap:

- `AgentHarness::prompt()`
- `AgentHarness::skill()`
- `AgentHarness::prompt_from_template()`
- `AgentHarness::compact()`
- `AgentHarness::navigate_tree()`
- Session storage append facade

Example:

```rust
trace_operation(
    "elph.agent.prompt",
    hashmap! {
        "session_id" => turn_state.session_id,
        "provider" => turn_state.model.provider,
        "model" => turn_state.model.id,
        "prompt_length" => text.len(),
    },
    execute_turn(turn_state, text, options),
).await
```

### elph-ai

Wrap common provider boundaries:

- `Models::stream_simple()`
- `Models::complete_simple()`

End/error payloads can include safe metadata:

- stop reason
- status code
- retry count
- input/output/total tokens
- cost total
- aborted/timeout flag

## Safety and redaction

Default payloads must be safe.

Safe by default:

- provider
- model
- API identifier
- session id
- entry type
- tool name
- status code
- stop reason
- token counts
- costs
- durations

Unsafe by default:

- prompts
- completions
- tool args
- tool results
- shell output
- file contents
- provider request payloads
- provider response bodies
- API keys
- headers

Content capture can be opt-in later with explicit redaction hooks.

## Listener behavior

Observability must never affect elph execution.

Subscriber errors should be swallowed or isolated. Harness hooks are control-plane and may affect execution; observability subscribers are passive and must not.

## User context

Users can associate arbitrary context with a turn:

```rust
run_with_elph_context(
    hashmap! {
        "user_id" => "u123",
        "org_id" => "acme",
        "region" => "eu",
    },
    || harness.prompt("fix this", None),
).await?;
```

Every emitted event inside that async chain includes the context.

An OTel adapter can map this to span attributes. A Sentry adapter can map it to Sentry context/spans.

## Interim approach

Until the dedicated observability crate exists, use:

- `tracing` spans in application binaries (`elph`, `eclaw`)
- `AgentHarness::subscribe` for agent lifecycle events
- `elph-ai` stream event hooks for provider timing

Do not add vendor-specific instrumentation inside `elph-agent` core.

## Thesis

Elph defines a stable, safe event contract. Adapters define where events go.

This makes ai/harness observable without binding core packages to OTel, Sentry, runtime-specific context APIs, or monkey-patching.
