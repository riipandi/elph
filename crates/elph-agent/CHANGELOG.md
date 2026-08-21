# Changelog

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## 0.0.28

### Extensions

- WASM host is **wasmi** (core module ABI, JSON in linear memory). wasmtime, Cranelift, and the WIT Component Model guest world are removed.
- Guests export `elph_init` / `elph_on_event` / `elph_execute_command` / `elph_execute_tool` and import `elph::{register_command,register_tool,subscribe,notify,confirm}`.
- Manifest field is `wasm` (not `component`). Target `wasm32-unknown-unknown`; WASI modules are rejected.

### Library contract

- MSRV 1.89, edition 2024. Package metadata: `documentation = "https://docs.rs/elph-agent"`.
- [`HostIdentity`] (`app_name` + `env_prefix`) on [`AgentOptions`]. Prefix applies to `{PREFIX}_PROMPT_ENCODING*`. MCP `{PREFIX}_AUTH_KEY` via `load_or_create_master_key_with_prefix`. Logging `{PREFIX}_DATA_DIR` / `{PREFIX}_LOG_*`.
- Defaults: `AgentBuilder` `app_name = "elph-agent"`, `env_prefix = "ELPH"`.
- Test-only MCP key helpers and `get_or_throw` / `get_or_undefined` are `#[doc(hidden)]`.
- Crate root is a prelude (`Agent`, `AgentOptions`, `HostIdentity`, tool constructors). Host APIs live on modules (`harness`, `session`, `compaction`, `collaboration`, `runtime`, `mcp`).
- Compaction cut-point helpers are `pub(crate)`; cover them with unit tests, not crate-root API.
- Turso/SQLite session backend and stores are feature `backend-turso` (included in `full`). In-memory / JSONL / session-dir backends stay available without it.
- `run_agent_loop` / `prompt` / `reset` return `AgentError`. Tool execute paths return `ToolError`.
- Consumer contract: [`docs/elph-agent.md`](../../docs/elph-agent.md).
