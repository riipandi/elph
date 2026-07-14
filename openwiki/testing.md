# Testing

Elph uses **cargo nextest** as the primary test runner with a mix of unit tests (colocated), integration tests (per-crate `tests/`), and E2E tests.

## Running tests

```sh
# All workspace tests
make test

# Specific crate
make test ARGS="-p elph-agent"
cargo nextest run -p elph-agent

# Single test
cargo nextest run -p elph-agent test_name

# With output
make test ARGS="-- --nocapture"

# Coverage (if installed)
make coverage
```

## Test suite by crate

### `elph-agent` (28 test files)

**Path**: `/crates/elph-agent/tests/`

| Test file                | What it covers                                      |
| ------------------------ | --------------------------------------------------- |
| `agent_loop.rs`          | Core agent turn loop, streaming, tool call cycle    |
| `agent.rs`               | Agent construction, event subscription              |
| `compaction.rs`          | Context compaction, summarization, token estimation |
| `e2e.rs`                 | End-to-end agent sessions                           |
| `encrypt_string.rs`      | MCP credential encryption/decryption                |
| `env.rs`                 | Local execution environment                         |
| `goals.rs`               | Goal creation, persistence, steering                |
| `harness_stream.rs`      | Harness streaming behavior                          |
| `harness.rs`             | AgentHarness construction and basic operation       |
| `mcp_deepwiki.rs`        | MCP deep integration test                           |
| `messages.rs`            | Message conversion utilities                        |
| `plan_mode.rs`           | Plan mode behavior                                  |
| `plugins.rs`             | WASM extension loading                              |
| `prompt_encoding.rs`     | TOON encoding/decoding                              |
| `prompt.rs`              | Prompt template loading and invocation              |
| `repo.rs`                | Session repository operations                       |
| `resource_formatting.rs` | Resource formatting for system prompt               |
| `serde_roundtrip.rs`     | Serialization round-trips                           |
| `session_tsid.rs`        | TSID generation for sessions                        |
| `session.rs`             | Session persistence and lifecycle                   |
| `skills.rs`              | Skill discovery, parsing, validation                |
| `storage.rs`             | Session storage backends                            |
| `subagent.rs`            | Subagent orchestration                              |
| `system_prompt.rs`       | System prompt assembly                              |
| `tools_fff.rs`           | fff-based file search tools                         |
| `tracing_http.rs`        | W3C traceparent header propagation                  |
| `truncate.rs`            | Result truncation                                   |
| `web_tools.rs`           | Web search and fetch tools                          |

> Tracing tests require the `tracing` feature: `cargo test -p elph-agent --features tracing`

### `elph-ai` (~12 test files)

**Path**: `/crates/elph-ai/tests/`

| Test file                        | What it covers                                   |
| -------------------------------- | ------------------------------------------------ |
| `bedrock_*.rs`                   | AWS Bedrock payload format, streaming, thinking  |
| `codex_websocket.rs`             | OpenAI Codex WebSocket transport                 |
| `cross_provider_handoff_live.rs` | Cross-provider session handoff                   |
| `faux_provider.rs`               | Faux provider behavior                           |
| `google_shared_thinking.rs`      | Google AI shared thinking                        |
| `http_proxy.rs`                  | HTTP proxy transport                             |
| `images_models.rs`               | Image generation models                          |
| `mistral_tool_schema.rs`         | Mistral tool schema format                       |
| `oauth_auth.rs`                  | OAuth authentication flows                       |
| `openai_*.rs`                    | OpenAI completions, tool choice, response images |
| `sse_abort.rs`                   | SSE abort handling                               |
| `tracing_http.rs`                | fastrace traceparent header injection            |

> Tracing tests require the `tracing` feature: `cargo test -p elph-ai --features tracing`

### `elph-core` (~4 test files)

**Path**: `/crates/elph-core/tests/`

Core utility tests — path resolution, logger, floppy memory store, tracing.

| Test file                | What it covers                                        |
| ------------------------ | ----------------------------------------------------- |
| `tracing_integration.rs` | `JsonlReporter`, `root_span`, init skip when disabled |

> Tracing tests require the `tracing` feature: `cargo test -p elph-core --features tracing`

### `elph-tui` (no tests — crate disabled)

The elph-tui crate is temporarily disabled. Former test files `tuie_shell.rs` and `agent_demo.rs` were removed along with the crate content.

### `elph` (2 test files)

**Path**: `/elph/tests/`

| Test file      | What it covers                        |
| -------------- | ------------------------------------- |
| `cli.rs`       | CLI subcommand parsing and execution  |
| `bootstrap.rs` | Platform bootstrap (paths, datastore) |

## Test patterns

### Faux provider

For testing agent behavior without real API calls, use the `faux_provider`:

```rust
use elph_ai::{faux_provider, faux_assistant_message, faux_text, faux_tool_call, FauxResponseStep};

let handle = faux_provider(RegisterFauxProviderOptions {
    model_id: "test-model".into(),
    responses: vec![
        FauxResponseStep::Delta(faux_assistant_message("Hello")),
        FauxResponseStep::Delta(faux_tool_call("read", json!({"path": "foo.txt"}))),
    ],
});
let models = elph_ai::builtin_models(Some(vec![handle]));
```

Source: `/crates/elph-ai/src/providers/faux.rs`

### In-memory session storage

For tests that don't need persistent sessions:

```rust
use elph_agent::session::InMemorySessionStorage;

let storage = InMemorySessionStorage::default();
```

### Tempfile environments

Use `tempfile` for sandboxed filesystem tests:

```rust
use tempfile::TempDir;

let dir = TempDir::new()?;
let env = Arc::new(LocalExecutionEnv::new(dir.path()));
```

## CI integration

**Source**: `/.github/workflows/_ci-app.yml`

CI runs:

1. `cargo check --workspace`
2. `cargo nextest run --no-fail-fast`
3. `cargo clippy --workspace -D warnings`

## Writing tests

### Guidelines

1. **Unit tests** — Colocated with the code they cover (standard Rust `#[cfg(test)]` pattern)
2. **Integration tests** — In each crate's `tests/` directory
3. **E2E tests** — In `crates/elph-agent/tests/e2e.rs` for agent runtime
4. **Use faux provider** — Never require real API keys for core tests
5. **Session tests** — Use `InMemorySessionStorage` to avoid filesystem dependencies
6. **Tool tests** — Use `tempfile::TempDir` environments
7. **Prompt encoding tests** — Test encoding/decoding round-trips

### What to test

| Area            | What to cover                                            |
| --------------- | -------------------------------------------------------- |
| Agent loop      | Turn lifecycle, tool iteration limit, streaming          |
| Session         | Create, fork, branch, resume, persistence                |
| Compaction      | Token estimation, summarization, cut-point detection     |
| MCP             | Connection, tool list, tool call, encryption, auth       |
| Skills          | Discovery, parsing, validation, system prompt formatting |
| Tools           | Each tool: execution, error handling, path resolution    |
| Prompt encoding | Encode/decode round-trip, heuristic detection            |
| CLI             | Subcommand parsing, flag handling, error output          |
| TUI             | Shell event loop, overlay state, transcript rendering    |

## Debugging

- **Session logs** — Sessions store events in `~/.elph/sessions/` — inspect with `elph session view`
- **MCP debug** — `RUST_LOG=debug` to see MCP protocol traffic
- **Prompt encoding** — Check `RUST_LOG=debug` for encoding decisions and savings ratios
- **Settings** — `elph doctor` shows resolved configuration
  ebug` for encoding decisions and savings ratios
- **Settings** — `elph doctor` shows resolved configuration
