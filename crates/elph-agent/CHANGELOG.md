# Changelog

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## 0.0.28

### Library contract

- MSRV 1.89, edition 2024. Package metadata: `documentation = "https://docs.rs/elph-agent"`.
- [`HostIdentity`] (`app_name` + `env_prefix`) on [`AgentOptions`]. Prefix applies to `{PREFIX}_PROMPT_ENCODING*`. MCP `{PREFIX}_AUTH_KEY` via `load_or_create_master_key_with_prefix`. Logging `{PREFIX}_DATA_DIR` / `{PREFIX}_LOG_*`.
- Defaults: `AgentBuilder` `app_name = "elph-agent"`, `env_prefix = "ELPH"`.
- Test-only MCP key helpers and `get_or_throw` / `get_or_undefined` are `#[doc(hidden)]`.
- Consumer contract: [`docs/elph-agent.md`](../../docs/elph-agent.md).
