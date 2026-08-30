# elph-agent consumer contract

`elph-agent` is the standalone agent runtime (loop, tools, sessions, MCP, harness). Built on [`elph-ai`](./elph-ai.md).

Version: `0.0.28`. MSRV: **1.89** (edition 2024). Docs: <https://docs.rs/elph-agent>

## What to depend on

| Surface | Role |
| --- | --- |
| `Agent`, `AgentOptions`, `AgentEvent`, `AgentError`, `HostIdentity` | Crate-root prelude |
| `simple_tool`, feature-gated `create_*_tool`, `ToolError` | Tools |
| `elph_agent::harness` | Session-backed orchestration |
| `elph_agent::session` | Persistence (SQL schemas, backends) |
| `elph_agent::mcp` (`mcp` feature) | MCP client |
| `elph_agent::compaction`, `collaboration`, `runtime`, … | Host APIs — import from the module, not the crate root |

TypeScript-port helpers (`get_or_throw`, `get_or_undefined`) and `*_for_tests` are `#[doc(hidden)]`.

## Host identity

```rust
use elph_agent::{AgentOptions, HostIdentity};

let options = AgentOptions {
    identity: Some(HostIdentity::new("myapp", "MYAPP")),
    ..Default::default()
};
```

| Field | Effect |
| --- | --- |
| `app_name` | Log file names, XDG `…/share/{app_name}/logs` |
| `env_prefix` | `{PREFIX}_PROMPT_ENCODING*`, `{PREFIX}_AUTH_KEY`, `{PREFIX}_DATA_DIR` |

Default prefix is `ELPH`. Logging uses [`AgentBuilder::env_prefix`] / [`AgentBuilder::app_name`] (defaults `ELPH` / `elph-agent`). Elph sets `app_name("elph")` and `env_prefix("ELPH")`.

## Features

`default = []`. Enable `mcp`, `builtin-tools`, `prompt-templates`, `tracing`, `backend-turso`, or `full`. Native command hooks are hosted by the `coding-agent` crate.

`backend-turso` enables Turso/SQLite session storage (`TursoSessionStorage`, `TursoSessionRepo`), the `datastore` helpers, and Turso-backed stores (turns, todos, goals, workers, session summaries, subagent graph). Without it, use `InMemorySessionStorage`, `JsonlSessionStorage`, or `SessionDirStorage`.

## Errors

`prompt` / `continue_run` / `reset` / `run_agent_loop` return [`AgentError`]. Tool execute functions return [`ToolError`] (`simple_tool` still accepts `anyhow` closures and maps them). Harness and session keep `AgentHarnessError` and `SessionError`. Stream token failures stay in-band on `elph_ai::StopReason`.

Prompt runs finalize their active-run state on normal completion, error, and
cancellation. If a prompt future is dropped while running, cleanup is
scheduled to cancel the run, release idle waiters, clear pending prompt
metadata, and return the harness to `Idle`.

Harness, MCP, session, compaction, and collaboration are **not** flattened at the crate root. Import from the module:

```rust
use elph_agent::harness::{AgentHarness, AgentHarnessOptions, FileSystem, Skill};
use elph_agent::mcp::{McpConfig, McpToolRegistry, load_or_create_master_key_with_prefix};
use elph_agent::session::CANONICAL_SESSION_SCHEMA_SQL;
use elph_agent::compaction::{estimate_context_tokens, should_compact};
```

## Compaction

`CompactionSettings::threshold_pct` triggers automatic compaction when estimated
context usage reaches the configured percentage, including the exact boundary.
The host performs the check before a prompt (including the prompt being
submitted) and after a successful turn. A zero context window disables the
automatic check because the model does not provide a usable limit.
When switching to a model with a smaller context window, the host estimates the
current history first and compacts it while the current model is still active,
before recording the model change.

`AgentHarness::compact` uses the harness defaults. Hosts that need a one-off
retention policy can call `AgentHarness::compact_with_settings`; its settings
apply only to that operation and do not mutate subsequent turns.

Compaction summarization inherits the harness provider timeout. When no timeout
is configured, it uses a 120-second safety timeout so a stalled provider cannot
leave the session busy indefinitely.
