# Observability

**Status: partially implemented** via [`fastrace`](https://crates.io/crates/fastrace) and [`logforth`](https://crates.io/crates/logforth).

Design lineage: [pi-agent observability](https://github.com/earendil-works/pi/blob/main/packages/agent/docs/observability.md). Elph uses a two-layer stack — structured **logging** and distributed **tracing** — without binding core crates to OpenTelemetry, Sentry, or any APM vendor.

## Architecture

| Layer   | Crate stack               | Output                                      |
| ------- | ------------------------- | ------------------------------------------- |
| Logging | `log` → `logforth`        | `{logs_dir}/{app}.jsonl` (rolling)          |
| Tracing | `fastrace`                | `{logs_dir}/{app}-traces.jsonl`             |
| Bridge  | `logforth::FastraceEvent` | Log events attached to the active span tree |

Logging and tracing are initialized together through `elph_core::logger::init()`. The returned [`LogGuard`](../../elph-core/src/logger/mod.rs) must live for the process lifetime; on drop it flushes async log writers and calls `fastrace::flush()`.

```rust
let init = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
    .env_prefix("ELPH")
    .app_name("elph")
    .logs_dir(paths.logs_dir())
    .build();

let _log_guard = elph_core::logger::init(init.logging);
```

`elph_core::trace::init()` runs inside `logger::init()` and installs a [`JsonlReporter`](../../elph-core/src/trace/reporter.rs) when tracing is enabled.

## Cargo features

The Cargo feature is named `tracing` for historical reasons. It enables `fastrace`, not the `tracing` crate.

| Crate         | Feature   | Default | Enables                                           |
| ------------- | --------- | ------- | ------------------------------------------------- |
| `elph-core`   | `tracing` | no      | `fastrace`, `fastrace-reqwest`, `JsonlReporter`   |
| `elph-ai`     | `tracing` | no      | Provider stream spans, HTTP trace propagation     |
| `elph-agent`  | `tracing` | no      | Harness/loop/tool/MCP spans (chains to above)     |
| `elph` binary | —         | always  | `tracing` on `elph-core`, `elph-ai`, `elph-agent` |

Library consumers opt in explicitly:

```toml
elph-agent = { version = "0.0", features = ["tracing", "mcp"] }
elph-ai = { version = "0.0", features = ["tracing"] }
elph-core = { version = "0.0", features = ["tracing"] }
```

Without the `tracing` feature, span macros compile to no-ops and `with_trace_headers()` returns the request unchanged.

## Environment variables

Resolved by [`LoggingOptions::resolve`](../../elph-core/src/logger/options.rs) via [`AgentBuilder`](../../elph-agent/src/builder.rs). The `elph` binary uses prefix `ELPH`.

| Variable                 | Default | Effect                                        |
| ------------------------ | ------- | --------------------------------------------- |
| `{PREFIX}_TRACE`         | on      | Set to `0` to disable trace file output       |
| `{PREFIX}_LOG_LEVEL`     | `info`  | `trace` / `debug` / `info` / `warn` / `error` |
| `{PREFIX}_LOG_FILE`      | on      | Set to `0` to disable rolling JSONL logs      |
| `{PREFIX}_LOG_ROTATION`  | `daily` | `hourly`, `daily`, or `weekly`                |
| `{PREFIX}_LOG_MAX_FILES` | —       | Cap retained rotated log files                |

Trace collection is skipped when `trace_enabled` is false, in unit tests (`cfg!(test)`), or when the reporter cannot be created (a warning is logged and execution continues).

## Trace output format

Each completed span is written as one JSON line:

```json
{
    "trace_id": "…",
    "span_id": "…",
    "parent_id": "…",
    "name": "elph.agent.turn",
    "begin_time_unix_ns": 1710000000000000000,
    "duration_ns": 123456789,
    "properties": { "model.id": "claude-sonnet-4" },
    "events": []
}
```

The reporter flushes on a one-second interval and on process shutdown.

## Span inventory

Spans use stable `elph.*` names. Instrumentation is gated behind `#[cfg_attr(feature = "tracing", fastrace::trace(…))]` or explicit `Span` helpers.

### Agent harness (`elph-agent`)

| Span name                  | Location                     | Notes                                 |
| -------------------------- | ---------------------------- | ------------------------------------- |
| `elph.agent.turn`          | `AgentHarness::prompt`       | Root of a user prompt turn            |
| `elph.agent.execute_turn`  | `execute_turn`               | Turn body after queue drain           |
| `elph.agent.loop`          | `run_agent_loop`             | Full agent loop for one turn          |
| `elph.agent.loop_continue` | loop continuation            | Follow-up iterations in the same turn |
| `elph.agent.tool_batch`    | tool batch dispatch          | Parallel tool call batch              |
| `elph.agent.tool`          | `execute_prepared_tool_call` | Single tool execution                 |

### MCP (`elph-agent`)

| Span name            | Location               |
| -------------------- | ---------------------- |
| `elph.mcp.connect`   | `connect_with_context` |
| `elph.mcp.call_tool` | MCP tool invocation    |

### Provider streaming (`elph-ai`)

| Span name        | Location                                        | Properties                                |
| ---------------- | ----------------------------------------------- | ----------------------------------------- |
| `elph.ai.stream` | `Models::lazy_stream` via `trace::spawn_stream` | `model.id`, `model.provider`, `model.api` |

Example trace tree for one prompt turn:

```text
elph.agent.turn
└─ elph.agent.execute_turn
   └─ elph.agent.loop
      ├─ elph.ai.stream          (model.id, model.provider, model.api)
      ├─ elph.agent.tool_batch
      │  └─ elph.agent.tool
      └─ elph.agent.loop_continue
         └─ elph.ai.stream
```

### HTTP trace propagation

When the `tracing` feature is enabled, outbound HTTP requests include W3C `traceparent` headers via `fastrace-reqwest`:

- `elph_ai::trace::with_trace_headers` — all provider API requests in `elph-ai`
- `elph_agent::trace::with_trace_headers` — MCP SSE/HTTP and web tools (`websearch`, `webfetch`)

Propagation requires an active local parent span (`Span::set_local_parent()` or `#[fastrace::trace]` on the calling async fn). Stream tasks use `FutureExt::in_span()` so spawned work stays `Send` without holding `LocalSpan` guards across `.await`.

## Harness lifecycle events

`AgentHarness::subscribe` emits control-plane lifecycle events (turn start/end, tool calls, provider hooks). These are separate from fastrace spans: subscribers can affect execution; trace collection is passive and must not.

Use harness events for UI and policy hooks. Use trace spans for latency analysis and cross-service correlation.

## Enabling tracing in downstream apps

1. Enable the `tracing` feature on `elph-core`, `elph-ai`, and/or `elph-agent` as needed.
2. Call `elph_core::logger::init()` early in `main`, keeping the `LogGuard` alive.
3. Set `{PREFIX}_TRACE` (omit or non-`0` to enable) and configure log directory via `AgentBuilder::logs_dir`.
4. Inspect `{app}-traces.jsonl` under the logs directory.

For custom root spans outside the harness, use `elph_core::trace::root_span("my.app.operation")`.

## Safety and redaction

Default span properties are metadata only. The implementation does **not** attach prompts, completions, tool arguments, file contents, or provider payloads to spans.

Safe by default:

- provider, model, API identifier
- span names and durations
- HTTP trace correlation IDs

Unsafe by default (not captured):

- prompts and completions
- tool args and results
- shell output and file contents
- provider request/response bodies
- API keys and auth headers

Opt-in content capture and redaction hooks remain future work.

## Tests

| Test crate   | File                           | Covers                                                  |
| ------------ | ------------------------------ | ------------------------------------------------------- |
| `elph-core`  | `tests/tracing_integration.rs` | `JsonlReporter`, `root_span`, `init` skip when disabled |
| `elph-core`  | `trace/reporter.rs` unit tests | JSONL line format                                       |
| `elph-ai`    | `tests/tracing_http.rs`        | `traceparent` header injection                          |
| `elph-agent` | `tests/tracing_http.rs`        | `traceparent` header injection                          |

Run with the `tracing` feature enabled:

```bash
cargo test -p elph-core --features tracing
cargo test -p elph-ai --features tracing
cargo test -p elph-agent --features tracing
```

## Future work

The original runtime-agnostic `ElphObservability` trait design (custom event bus, user context propagation, OTel/Sentry adapters) is **not** implemented. Remaining gaps:

| Area                 | Planned span / capability                                                 |
| -------------------- | ------------------------------------------------------------------------- |
| Harness entry points | `elph.agent.skill`, `elph.agent.prompt_template`, `elph.agent.compaction` |
| Session I/O          | `elph.session.append_entry`, `elph.session.read`, `elph.session.write`    |
| Provider detail      | `elph.ai.provider.request`, retry/first-token/usage events                |
| User context         | `run_with_elph_context` — arbitrary key/value on every event              |
| Adapters             | OTel span export, Sentry bridge, custom `Reporter` implementations        |
| Redaction            | Opt-in payload capture with explicit scrubbing hooks                      |

Until those exist, fastrace JSONL plus harness `subscribe` events are the supported observability surface.

## Thesis

Elph emits stable, safe span names and structured logs. External tooling can ingest `{app}-traces.jsonl` and convert span trees into OTel, dashboards, or APM views without vendor code inside `elph-agent` or `elph-ai` core.
