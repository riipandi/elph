# Observability

**Status: implemented** via [`log`](https://crates.io/crates/log) + [`logforth`](https://crates.io/crates/logforth) (dispatch, filters, async file, fastrace bridge) and [`serde-jsonlines`](https://crates.io/crates/serde-jsonlines) (typed JSONL records).

Design lineage: [pi-agent observability](https://github.com/earendil-works/pi/blob/main/packages/agent/docs/observability.md). Elph uses a two-layer stack — structured **logging** and distributed **tracing** — without binding core crates to OpenTelemetry, Sentry, or any APM vendor.

## Architecture

| Layer   | Crate stack                                      | Output                                      |
| ------- | ------------------------------------------------ | ------------------------------------------- |
| Logging | `log` → `logforth` → typed `LogRecord` JSONL     | `{logs_dir}/{app}.jsonl` (rolling)          |
| Tracing | `fastrace` + non-blocking `JsonlReporter`        | `{logs_dir}/{app}-traces.jsonl`             |
| Crash   | panic hook → `CrashRecord` JSONL                 | `{logs_dir}/crash-YYMMDDhh.jsonl` (UTC)     |
| Bridge  | `logforth::FastraceEvent`                        | Log events attached to the active span tree |

Library crates emit through the `log` facade. Only the process (`elph` / an embedder) calls [`logger::init`](../src/logger/mod.rs). The returned [`LogGuard`](../src/logger/mod.rs) must live for the process lifetime; on drop it flushes async log writers and `fastrace::flush()`.

Configure with [`LoggingOptions::builder`](../src/logger/options.rs). Merge order: defaults → `settings.json` `logging` → `{PREFIX}_LOG_*` / `{PREFIX}_TRACE` (env wins). File-open failure degrades (stderr warning, no file appender) instead of panicking.

Third-party libraries that write straight to fd 2 — the MCP (rmcp) client — are redirected to `{logs_dir}/mcp.log`. See [`logger::redirect_stderr_to_file`](../src/logger/mod.rs).

```rust
let init = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
    .env_prefix("ELPH")
    .app_name("elph")
    .logs_dir(paths.logs_dir())
    .logging_settings(settings.logging)
    .console_enabled(false)
    .build();

let _log_guard = elph_agent::logger::init(init.logging);
```

Tracing initialization runs inside the logging initializer and installs a [`JsonlReporter`](../src/trace/reporter.rs) when tracing is enabled. `elph-ai` only receives `trace::set_enabled` so stream/HTTP helpers attach to the same reporter.

### App log record

Each file line is one `LogRecord`:

```json
{
    "ts": "2026-08-18T12:00:00.123Z",
    "level": "INFO",
    "target": "elph_agent::session",
    "module": "elph_agent::session",
    "file": "session/mod.rs",
    "line": 225,
    "message": "release session lease",
    "thread": "main"
}
```

Timestamps are UTC. Optional `kv` is omitted when empty. Prompts, completions, tool args, and credentials are not attached.

### Crash logs

Panics append one `CrashRecord` to `{logs_dir}/crash-YYMMDDhh.jsonl` (2-digit year + month + day + UTC hour). Example: `2026-08-18T14:07:00Z` → `crash-26081814.jsonl`. Multiple panics in the same hour append extra lines.

## Cargo features

The Cargo feature is named `tracing` for historical reasons. It enables `fastrace`, not the `tracing` crate.

| Crate         | Feature   | Default | Enables                                           |
| ------------- | --------- | ------- | ------------------------------------------------- |
| `elph-ai`     | `tracing` | no      | Provider stream spans, HTTP trace propagation     |
| `elph-agent`  | `tracing` | no      | Harness/loop/tool/MCP spans (chains to above)     |
| `elph` binary | —         | always  | `tracing` on `elph-ai`, `elph-agent` |

Library consumers opt in explicitly:

```toml
elph-agent = { version = "0.0", features = ["tracing", "mcp"] }
elph-ai = { version = "0.0", features = ["tracing"] }
```

Without the `tracing` feature, span macros compile to no-ops and `with_trace_headers()` returns the request unchanged.

## Environment variables

Resolved by [`LoggingOptions::builder`](../src/logger/options.rs) via [`AgentBuilder`](../src/builder.rs). The `elph` binary uses prefix `ELPH`. Env vars, when set, override `settings.json` `logging`.

| Variable                  | Default | Effect                                                                                          |
| ------------------------- | ------- | ----------------------------------------------------------------------------------------------- |
| `{PREFIX}_TRACE`          | on      | Set to `0`, `false`, `off`, or `no` to disable tracing                                          |
| `{PREFIX}_LOG_LEVEL`      | `info`  | rustlog spec: `info` or `elph_agent=debug,elph_ai=warn`                                         |
| `{PREFIX}_LOG_FILE`       | on      | Set to `0` to disable rolling JSONL logs                                                        |
| `{PREFIX}_LOG_ROTATION`   | `daily` | `hourly`, `daily`, or `size`                                                                    |
| `{PREFIX}_LOG_MAX_FILES`  | —       | Cap retained rotated log files                                                                  |
| `{PREFIX}_LOG_MAX_BYTES`  | —       | Size trigger (used with `size` rotation; default 10 MiB when rotation is `size`)                |
| `{PREFIX}_LOG_CONSOLE`    | off     | Set to `1` to also write human text on stderr (the `elph` binary keeps this off for TUI/pipes)  |

`settings.json` group (optional; restart required):

```json
"logging": {
    "level": "info",
    "file": true,
    "rotation": "daily",
    "maxFiles": null,
    "maxBytes": null,
    "trace": true
}
```

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
| `elph.agent.turn`            | `AgentHarness::prompt`            | Root of a user prompt turn              |
| `elph.agent.skill`           | `AgentHarness::skill`             | `skill.name`                            |
| `elph.agent.prompt_template` | `AgentHarness::prompt_from_template` | `template.name`                      |
| `elph.agent.execute_turn`    | `execute_turn`                    | Turn body after queue drain             |
| `elph.agent.loop`            | `run_agent_loop`                  | Full agent loop for one turn            |
| `elph.agent.loop_continue`   | loop continuation                 | Follow-up iterations in the same turn   |
| `elph.agent.tool_batch`      | tool batch dispatch               | Parallel tool call batch                |
| `elph.agent.tool`            | `execute_prepared_tool_call`      | `tool.name`                             |
| `elph.agent.compaction`      | `compact`                         | `model.id`, `model.provider`            |
| `elph.agent.subagent_spawn`  | `AgentControl::spawn_agent`       | `subagent.id`                           |

### Markdown (`rendown`)

Library crate: no logger init, no fastrace spans. Emits `log` for mermaid render failure, syntax-highlight fallback, and ANSI write/stream I/O errors. Source markdown is never logged.

Filter: `ELPH_LOG_LEVEL=rendown=debug`.

### Memory (`floppy`)

Library crate: no logger init, no fastrace spans. Emits `log` for store open/init/close, migrations, embedder load, decay/consolidate/purge/flush, and embed fallbacks. Memory **content** and search queries are never logged.

Filter: `ELPH_LOG_LEVEL=floppy=debug`.

### Host / CLI (`elph` binary, `coding-agent`)

The binary initializes logging, then emits `log` records for process lifecycle. Filter with `ELPH_LOG_LEVEL=elph=debug`.

- CLI dispatch (`cli start command=…`), TUI launch, headless `elph run`
- Settings load/save
- Session pin/delete
- `/reload` workspace summary
- All `cli_error` paths (`error:` on stderr + JSONL)

Prompt text is never logged.

### Terminal UI (`elph-tui`)

`elph-tui` is a widget library: it does **not** initialize the logger and has no fastrace spans. It emits `log` records for I/O and config edges only (clipboard, theme, QR encode, CLI progress interrupt, paste size). Render/keystroke paths stay silent.

### MCP (`elph-agent`)

| Span name            | Location               |
| -------------------- | ---------------------- |
| `elph.mcp.connect`   | `connect_with_context` |
| `elph.mcp.call_tool` | MCP tool invocation    |

### Provider streaming (`elph-ai`)

| Span name                | Location                                              | Properties / events                                      |
| ------------------------ | ----------------------------------------------------- | -------------------------------------------------------- |
| `elph.ai.stream`         | `Models::lazy_stream` via `trace::spawn_stream`       | `model.id`, `model.provider`, `model.api`; event `first_token` |
| `elph.ai.http`           | `send_with_resilience` / `send_with_resilience_retry` | `provider.id`; event `retry`                             |
| `elph.ai.auth`           | `resolve_provider_auth`                               | `provider.id`                                            |
| `elph.ai.oauth.login`    | `oauth_provider_login`                                | `provider.id`                                            |
| `elph.ai.oauth.refresh`  | `refresh_oauth_token`                                 | `provider.id`                                            |
| `elph.ai.images`         | `ImagesModels::generate_images`                       | `model.id`, `model.provider`                             |
| `elph.ai.websocket`      | `connect_websocket_with_proxy`                        | `ws.host`, `ws.port`                                     |

Example trace tree for one prompt turn:

```text
elph.agent.turn
└─ elph.agent.execute_turn
   └─ elph.agent.loop
      ├─ elph.ai.stream          (model.id, model.provider, model.api)
      │  ├─ elph.ai.auth
      │  └─ elph.ai.http         (retry events)
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

1. Enable the `tracing` feature on `elph-ai`, and/or `elph-agent` as needed.
2. Call `AgentBuilder` early in `main`, keeping the `LogGuard` alive from `init.logging.init()`.
3. Set `{PREFIX}_TRACE` (omit or non-`0` to enable) and configure log directory via `AgentBuilder::logs_dir`.
4. Inspect `{app}-traces.jsonl` under the logs directory.

For custom root spans outside the harness, use `elph_agent::trace::root_span("my.app.operation")`.

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

### Provider HTTP logs (`elph-ai`)

| Event | Level | Fields |
| ----- | ----- | ------ |
| Successful provider HTTP | `debug` | status, `provider=` — no body |
| Stream complete | `info` | `provider=`, `model=`, token usage (`in`/`out`/`cache_read`/`cache_write`/`total`), stop `reason` |
| HTTP 4xx/5xx | `warn` | status, `provider=`, short snippet from JSON `error`/`message`/`code`/`type` (≤160 chars) |

Non-JSON error bodies log as `(non-json error body)` so HTML or echoed prompts never land in the file. The `anyhow` error returned to callers still includes the truncated (4000-char) body.

Opt-in content capture and redaction hooks remain future work.

## Tests

| Test crate   | File                           | Covers                                                  |
| ------------ | ------------------------------ | ------------------------------------------------------- |
| `elph-agent` | `trace/reporter.rs` unit tests | JSONL line format                                       |
| `elph-ai`    | `tests/tracing_http.rs`        | `traceparent` header injection                          |
| `elph-agent` | `tests/tracing_http.rs`        | `traceparent` header injection                          |

Run with the `tracing` feature enabled:

```sh
cargo test -p elph-ai --features tracing
cargo test -p elph-agent --features tracing
```

## Future work

The original runtime-agnostic `ElphObservability` trait design (custom event bus, user context propagation, OTel/Sentry adapters) is **not** implemented. Remaining gaps:

| Area                 | Planned span / capability                                                 |
| -------------------- | ------------------------------------------------------------------------- |
| Harness entry points | remaining hook/plan-mode spans |
| Session I/O          | `elph.session.append_entry`, `elph.session.read`, `elph.session.write`    |
| Provider detail      | usage token fields on fastrace stream spans (JSONL usage log is implemented) |
| User context         | `run_with_elph_context` — arbitrary key/value on every event              |
| Adapters             | OTel span export, Sentry bridge, custom `Reporter` implementations        |
| Redaction            | Opt-in payload capture with explicit scrubbing hooks                      |

Until those exist, fastrace JSONL plus harness `subscribe` events are the supported observability surface.

## Thesis

Elph emits stable, safe span names and structured logs. External tooling can ingest `{app}-traces.jsonl` and convert span trees into OTel, dashboards, or APM views without vendor code inside `elph-agent` or `elph-ai` core.
