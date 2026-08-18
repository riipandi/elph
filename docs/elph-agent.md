# elph-agent consumer contract

`elph-agent` is the standalone agent runtime (loop, tools, sessions, MCP, harness). Built on [`elph-ai`](./elph-ai.md).

Version: `0.0.28`. MSRV: **1.89** (edition 2024). Docs: <https://docs.rs/elph-agent>

## What to depend on

| Surface | Role |
| --- | --- |
| `Agent`, `AgentOptions`, `AgentEvent` | Low-level loop |
| `AgentHarness`, `AgentHarnessOptions` | Session-backed orchestration |
| `HostIdentity` | App name + env prefix (not process-global) |
| `simple_tool`, feature-gated `create_*_tool` | Tools |
| `InMemorySessionStorage` / `JsonlSessionStorage` / Turso backends | Persistence |
| `mcp` feature | MCP client |

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

`default = []`. Enable `mcp`, `builtin-tools`, `extensions`, `prompt-templates`, `tracing`, or `full`.

Turso session backends are always compiled in this version (not feature-gated yet).

## Errors

Harness and session types have typed errors (`AgentHarnessError`, `SessionError`). The loop still uses `anyhow` in several tool/host paths.
