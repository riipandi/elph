---
type: Concept
title: Testing — Test Patterns and Strategies
description: Elph's testing patterns — unit tests, integration tests, live provider tests, and test harness tools
tags: [testing, integration-tests, unit-tests, live-tests, harness]
---

# Testing

## Test Organization

Tests are organized across the workspace. See [Architecture Overview](architecture/overview.md) for the crate structure, and [Source Map](architecture/source-map.md) for test file locations.

### Unit Tests

Located in the same file as the implementation, using `#[cfg(test)]` modules. This follows the convention documented in `AGENTS.md`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // test internal logic directly
}
```

### Integration Tests

Each crate has its own `tests/` directory:

| Crate        | Test Files                           | Focus                                                               |
| ------------ | ------------------------------------ | ------------------------------------------------------------------- |
| `elph-agent` | 30 test files                        | Agent loop, compaction, harness, goals, MCP, skills, sessions, etc. |
| `elph-ai`    | ~15 test files                       | Provider adapters, auth, message transformation, tool schemas       |
| `elph-tui`   | ~5 test files                        | UI component tests                                                  |
| `elph`       | `tests/bootstrap.rs`, `tests/cli.rs` | Bootstrap, CLI subcommands                                          |

## Test Patterns

### Common Module

All `elph-agent` integration tests share a `mod common;` module. It provides:

- `simple_tool()` — creates `AgentTool` from `elph_ai::Tool` definition
- `llm_message_to_agent()` — translates LLM messages to agent messages
- `user_agent_message()` — user message helper

### Faux Provider

Tests use `FauxProviderHandle` from `elph-ai`:

```rust
use elph_ai::faux_provider;
use elph_ai::faux_assistant_message;
use elph_ai::faux_text;
use elph_ai::faux_tool_call;
use elph_ai::FauxResponseStep;

// Create a faux provider that returns predefined responses
let (provider, handle) = faux_provider();
handle.add_step(FauxResponseStep::Text("Hello world"));
```

### Agent Loop Tests

From `crates/elph-agent/tests/agent_loop.rs` (50,377 bytes):

- Tests `run_agent_loop()` and `run_agent_loop_continue()`
- Tests event lifecycle: `AgentStart → TurnStart → MessageEnd → ToolExecution* → TurnEnd`
- Tests tool execution modes (background, visible, error handling)
- Tests `QueueMode` (normal, replace, background)

### Compaction Tests

From `crates/elph-agent/tests/compaction.rs` (41,052 bytes):

- Tests `compact()`, `should_compact()`, `calculate_context_tokens()`
- Tests `find_cut_point()`, `serialize_conversation()`, `estimate_tokens()`
- Tests `generate_summary()`, `prepare_compaction()`

### E2E Harness Tests

From `crates/elph-agent/tests/e2e.rs` (22,353 bytes):

- Tests `AgentHarness` with `InMemorySessionStorage`
- Tests `AgentHarnessResources`, `SystemPrompt`, `ToolExecutionMode`
- Uses `tempfile::TempDir` for filesystem isolation

## Live Provider Tests

Located in `crates/elph-ai/tests/`:

- `e2e_live.rs` — OpenAI, Anthropic, Gemini streaming tests
- `abort_live.rs` — Abort/cancellation tests
- `cross_provider_handoff_live.rs` — Provider switching
- `openrouter_cache_write_live.rs` — OpenRouter cache
- `tool_call_id_normalization_live.rs` — Tool call ID normalization

Live tests are marked `#[ignore = "requires X_API_KEY"]` and guarded by `has_env()`:

```rust
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_stream_returns_assistant_content() {
    if !has_env("OPENAI_API_KEY") {
        return;
    }
    // ...
}
```

Run them with:

```sh
cargo test -p elph-ai --test e2e_live -- --ignored
```

## Makefile Test Targets

```sh
make test                      # All workspace tests (cargo nextest)
make test-elph                 # elph-ai + elph + elph-agent (with full features)
make test-elph-tui             # elph-tui tests
make coverage                  # With cargo-llvm-cov
```

## Source References

- `crates/elph-agent/tests/` — 30 integration test files
- `crates/elph-ai/tests/` — provider adapter tests + live tests
- `elph/tests/bootstrap.rs` — home bootstrapping tests
- `elph/tests/cli.rs` — CLI subcommand tests
- `crates/elph-agent/src/tools/types.rs` — `AgentTool`, `AgentToolResult`
- `crates/elph-ai/src/providers/faux/` — `FauxProviderHandle`, `FauxResponseStep`
