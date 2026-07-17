# Observability

**Status: partially implemented** via [`fastrace`](https://crates.io/crates/fastrace) and [`logforth`](https://crates.io/crates/logforth).

Design lineage: [pi-agent observability](https://github.com/earendil-works/pi/blob/main/packages/agent/docs/observability.md). Elph uses a two-layer stack — structured **logging** and distributed **tracing** — without binding core crates to OpenTelemetry, Sentry, or any APM vendor.

## Architecture

| Layer   | Crate stack               | Output                                      |
| ------- | ------------------------- | ------------------------------------------- |
| Logging | `log` → `logforth`        | `{logs_dir}/{app}.jsonl` (rolling)          |
| Tracing | `fastrace`                | `{logs_dir}/{app}-traces.jsonl`             |
| Bridge  | `logforth::FastraceEvent` | Log events attached to the active span tree |

Logging is initialized by the product binary through `elph::logger::init()`. The returned [`LogGuard`](../../../elph/src/logger/mod.rs) must live for the process lifetime; on drop it flushes async log writers and calls `fastrace::flush()`.

```rust
let init = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
    .env_prefix("ELPH")
    .app_name("elph")
    .logs_dir(paths.logs_dir())
    .build();

let _log_guard = elph::logger::init(init.logging);
```

`elph_ai::trace::init()` runs inside `logger::init()` and installs a [`JsonlReporter`](../../elph-ai/src/trace/reporter.rs) when tracing is enabled. Process-global span state lives in `elph-ai` so both the provider layer and agent runtime share the same enable flag.

## Cargo features

The Cargo feature is named `tracing` for historical reasons. It enables `fastrace`, not the `tracing` crate.

| Crate         | Feature   | Default | Enables                                           |
| ------------- | --------- | ------- | ------------------------------------------------- |
| `elph-ai`     | `tracing` | no      | `JsonlReporter`, provider stream spans, HTTP W3C  |
| `elph-agent`  | `tracing` | no      | Harness/loop/tool/MCP spans (chains to `elph-ai`) |
| `elph` binary | —         | always  | `tracing` on `elph-ai` and `elph-agent`           |

Library consumers opt in explicitly:

```toml
elph-agent = { version = "0.0", features = ["tracing", "mcp"] }
elph-ai = { version = "0.0", features = ["tracing"] }
```

Without the `tracing` feature, span macros compile to no-ops and `with_trace_headers()` returns the request unchanged.

## Environment variables

Resolved by [`LoggingOptions::resolve`](../src/logger_options.rs) via [`AgentBuilder`](../src/builder.rs). The `elph` binary uses prefix `ELPH`.

| Variable                 | Default | Effect                                                                                             |
| ------------------------ | ------- | -------------------------------------------------------------------------------------------------- |
| `{PREFIX}_TRACE`         | on      | Set to `0`, `false`, `off`, or `no` to disable tracing (file output, log bridge, HTTP propagation) |
| `{PREFIX}_LOG_LEVEL`     | `info`  | `trace` / `debug` / `info` / `warn` / `error`                                                      |
| `{PREFIX}_LOG_FILE`      | on      | Set to `0` to disable rolling JSONL logs                                                           |
| `{PREFIX}_LOG_ROTATION`  | `daily` | `hourly`, `daily`, or `weekly`                                                                     |
| `{PREFIX}_LOG_MAX_FILES` | —       | Cap retained rotated log files                                                                     |

## Enabling for library embeds

1. Enable the `tracing` feature on `elph-ai` and/or `elph-agent` as needed.
2. Resolve `LoggingOptions` via `AgentBuilder` (or construct them directly).
3. Call `elph_ai::trace::init(logs_dir, app_name, enabled)` (or use the product `logger::init` when embedding the full app stack).
