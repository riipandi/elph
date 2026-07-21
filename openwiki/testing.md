---
type: Guide
title: Testing
description: Test organization, running tests, key test areas for each crate, and testing patterns used in the Elph workspace.
tags: [testing, ci, coverage, cargo-nextest]
resource: /
---

# Testing

Elph uses `cargo nextest` for test execution (run with `make test` or `cargo nextest run`). Unit tests are colocated with source code; integration tests live in each crate's `tests/` directory.

## Running tests

```sh
# All tests
make test                    # cargo nextest run

# Specific package
cargo nextest run -p elph-agent  # agent tests
cargo nextest run -p elph-ai     # AI model tests
cargo nextest run -p elph-tui    # TUI tests
cargo nextest run -p elph        # binary integration tests

# With output (for debugging)
cargo nextest run --nocapture

# Specific test
cargo nextest run -p elph-ai --test anthropic_thinking

# Check compilation only (fast)
make check
```

## Test layout by crate

### `elph-agent` tests — `/crates/elph-agent/tests/`

The largest test suite (30+ test files). Key areas:

| Test file                         | Covers                                                         |
| --------------------------------- | -------------------------------------------------------------- |
| `harness.rs`                      | Agent harness: creation, configuration, basic turn cycles      |
| `harness_stream.rs`               | Stream API: event emits during harness execution               |
| `agent.rs`                        | Agent struct: queue, run, state transitions                    |
| `agent_loop.rs`                   | Runtime loop: turn iteration, tool calls                       |
| `session.rs`, `session_kalid.rs`  | Session storage: create, resume, metadata                      |
| `storage.rs`                      | SessionDirStorage, InMemorySessionStorage, TursoSessionStorage |
| `compaction.rs`                   | History compaction: triggers, summarization, branch merge      |
| `goals.rs`                        | Goal lifecycle: create, complete, fail, budgets                |
| `subagent.rs`                     | Subagent spawning, depth limits, shared registry               |
| `skills.rs`                       | Skill loading and system prompt formatting                     |
| `prompt.rs`, `prompt_template.rs` | Prompt assembly, MiniJinja templates                           |
| `prompt_encoding.rs`              | TOON encoding: compression, delimiter config                   |
| `plan_mode.rs`                    | Collaboration: plan mode restrictions                          |
| `mcp_deepwiki.rs`                 | MCP client connectivity with DeepWiki server                   |
| `tools_fff.rs`                    | FFF search tool                                                |
| `web_tools.rs`                    | Web fetch and search tools                                     |
| `messages.rs`                     | Message types, shell output formatting                         |
| `serde_roundtrip.rs`              | Serialization roundtrip for all agent types                    |
| `truncate.rs`                     | Text truncation utility                                        |
| `tracing_http.rs`                 | HTTP trace header propagation                                  |
| `env.rs`                          | Local execution environment                                    |
| `e2e.rs`                          | End-to-end: full agent execution                               |
| `plugins.rs`                      | WASM plugins                                                   |
| `defaults.rs`                     | Default configuration                                          |
| `resource_formatting.rs`          | Resource formatting                                            |
| `system_prompt.rs`                | System prompt building                                         |
| `repo.rs`                         | Repository tool operations                                     |
| `encrypt_string.rs`               | MCP OAuth encryption                                           |
| `common/mod.rs`                   | Shared test utilities                                          |

### `elph-ai` tests — `/crates/elph-ai/tests/`

Provider-specific and protocol-level tests (40+ test files). Key areas:

| Test file                               | Covers                                            |
| --------------------------------------- | ------------------------------------------------- |
| `providers.rs`                          | Provider resolution and configuration             |
| `api_registry.rs`                       | API registry and routing                          |
| `faux_provider.rs`                      | Mock provider (used by other tests)               |
| `anthropic_thinking.rs`                 | Anthropic extended thinking                       |
| `anthropic_temperature.rs`              | Anthropic temperature parameter                   |
| `anthropic_sse_parsing.rs`              | Anthropic SSE response parsing                    |
| `anthropic_empty_thinking_signature.rs` | Edge case: empty thinking blocks                  |
| `bedrock_convert_messages.rs`           | AWS Bedrock message conversion                    |
| `bedrock_thinking_payload.rs`           | Bedrock thinking payload format                   |
| `bedrock_endpoint_resolution.rs`        | Bedrock endpoint resolution                       |
| `bedrock_custom_headers.rs`             | Bedrock custom HTTP headers                       |
| `google_shared_gemini3.rs`              | Google Gemini 3 integration                       |
| `google_shared_thinking.rs`             | Google thinking/thinking                          |
| `google_shared_convert_tools.rs`        | Google tool format conversion                     |
| `openai_compat.rs`                      | OpenAI-compatible API testing                     |
| `openai_responses_convert.rs`           | OpenAI Responses API conversion                   |
| `openai_responses_empty_tool_result.rs` | Empty tool results handling                       |
| `openai_responses_partial_json.rs`      | Partial JSON parsing                              |
| `openai_responses_terminal_event.rs`    | Terminal events                                   |
| `openai_completions_*.rs`               | Various OpenAI completion tests                   |
| `mistral_reasoning_mode.rs`             | Mistral reasoning mode                            |
| `mistral_tool_schema.rs`                | Mistral tool schema format                        |
| `codex_websocket.rs`                    | OpenAI Codex WebSocket transport                  |
| `codex_websocket_proxy.rs`              | Codex proxy support                               |
| `github_copilot_headers.rs`             | GitHub Copilot auth headers                       |
| `oauth_auth.rs`                         | OAuth 2.1 + PKCE flow                             |
| `cache_retention.rs`                    | Prompt cache control                              |
| `images_models.rs`                      | Image generation models                           |
| `http_proxy.rs`                         | HTTP proxy support                                |
| `retry.rs`                              | Retry logic                                       |
| `overflow.rs`                           | Overflow handling                                 |
| `validation.rs`                         | Input validation                                  |
| `abort.rs`, `abort_live.rs`             | Request cancellation                              |
| `sse_abort.rs`                          | SSE stream cancellation                           |
| `e2e_live.rs`                           | Live provider E2E (requires credentials)          |
| `cross_provider_handoff_live.rs`        | Cross-provider handoff (requires credentials)     |
| `tool_call_id_normalization_live.rs`    | Tool call ID normalization (requires credentials) |
| `openrouter_cache_write_live.rs`        | OpenRouter cache (requires credentials)           |
| `openrouter_images.rs`                  | OpenRouter image support                          |
| `tracing_http.rs`                       | HTTP trace propagation                            |
| `error_body.rs`                         | Error response body parsing                       |
| `transform_messages.rs`                 | Message transformation                            |
| `unicode_surrogate.rs`                  | Unicode surrogate handling                        |
| `stream_without_api_key.rs`             | Error handling without API key                    |
| `azure_openai_base_url.rs`              | Azure base URL configuration                      |
| `common/mod.rs`                         | Shared test utilities (includes `FauxProvider`)   |

### `elph-tui` tests — `/crates/elph-tui/tests/`

Component-level tests (14 test files):

| Test file               | Covers                       |
| ----------------------- | ---------------------------- |
| `color.rs`              | Color parsing and conversion |
| `text_input_layout.rs`  | Text input layout            |
| `transcript_layout.rs`  | Transcript layout rendering  |
| `textarea.rs`           | Textarea component           |
| `text_editing.rs`       | Text editing operations      |
| `scroll.rs`             | Scroll behavior              |
| `types.rs`              | Shared types serialization   |
| `utils.rs`              | Utility functions            |
| `components_render.rs`  | Component rendering          |
| `components_props.rs`   | Component props handling     |
| `components_mock.rs`    | Component mock rendering     |
| `components_helpers.rs` | Component test helpers       |
| `coverage_gaps.rs`      | Coverage gap detection       |
| `coverage_helpers.rs`   | Coverage utility helpers     |

### `elph-core` tests — `/crates/elph-core/tests/`

| Test file                | Covers                          |
| ------------------------ | ------------------------------- |
| `tracing_integration.rs` | Distributed tracing integration |

### `elph-exec` tests — `/crates/elph-exec/tests/`

| Test file  | Covers              |
| ---------- | ------------------- |
| `shell.rs` | Shell/PTY execution |

### `elph` (binary) tests — `/elph/tests/`

| Test file      | Covers                                       |
| -------------- | -------------------------------------------- |
| `bootstrap.rs` | Full app bootstrap, TUI initialization       |
| `cli.rs`       | CLI argument parsing and subcommand dispatch |

## Testing patterns

### Mock providers

The `FauxProvider` (`crates/elph-ai/src/providers/faux.rs`) provides a deterministic mock LLM provider for testing agent loops without real API calls.

### Live tests

Tests with `_live` suffix require real provider credentials. These are typically gated with `#[cfg(feature = "full")]` or `#[ignore]` annotations.

### Test organization principles

Per `/docs/codebase-layout.md`:

- **Unit tests**: colocated with the code they cover
- **Integration tests**: in each crate's `tests/` directory
- **File size**: prefer modules under ~400 lines; split by concern

### CI pipeline

GitHub Actions runs in `.github/workflows/`:

1. **Format check** — `cargo fmt --check`
2. **Lint** — `cargo clippy --workspace -D warnings`
3. **Test** — `cargo nextest run --workspace`
4. **Build** — `cargo build --workspace`

Uses `sccache` for compilation caching across CI runs.

## Tips for writing tests

- Use `FauxProvider` for agent loop tests without real API calls
- Session tests work with `InMemorySessionStorage` or a temp directory
- MCP tests use a local stdio-based test server or mock
- For TUI component tests, use the mock rendering context from `elph-tui/tests/components_mock.rs`
- To test a specific feature, check the Cargo feature flags in `crates/elph-agent/Cargo.toml`
