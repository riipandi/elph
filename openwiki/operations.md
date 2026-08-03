---
type: Concept
title: Operations — CLI, Configuration, and Observability
description: Elph operations guide — CLI subcommands, ELPH_ environment variables, config paths, Makefile targets, and observability
tags: [operations, cli, configuration, observability, environment-variables]
---

# Operations

See [Architecture Overview](architecture/overview.md) for the system architecture, [Source Map](architecture/source-map.md) for the module layout, and [Agent Loop](workflows/agent-loop.md) for the turn cycle.

## CLI Subcommands

The `elph` binary provides 19 subcommands (from `elph/src/cli/mod.rs`):

| Subcommand    | Alias | Description                                                                        |
| ------------- | ----- | ---------------------------------------------------------------------------------- |
| `acp`         | —     | Agent Client Protocol server over stdio                                            |
| `codegraph`   | —     | Semantic code index + impact (`build`/`update`/`status`/`search`/`impact`/`purge`) |
| `completions` | —     | Generate shell completion scripts                                                  |
| `doctor`      | —     | Show discovered configuration                                                      |
| `export`      | —     | Export session transcript/archive                                                  |
| `import`      | —     | Import sessions                                                                    |
| `mcp`         | —     | Manage MCP server configurations                                                   |
| `memory`      | `mem` | Inspect/manage agent memory (floppy)                                               |
| `models`      | —     | List available models                                                              |
| `extensions`  | `ext` | Manage Elph extensions                                                             |
| `provider`    | —     | Manage AI providers and credentials                                                |
| `run`         | —     | Run a prompt non-interactively                                                     |
| `server`      | —     | Local Elph REST+WS+Web UI server                                                   |
| `session`     | —     | List/search/restore sessions                                                       |
| `stats`       | —     | Token usage and cost statistics                                                    |
| `tools`       | —     | List available agent tools                                                         |
| `update`      | —     | Check for updates or install a version                                             |
| `version`     | —     | Print version                                                                      |
| `worktree`    | —     | Manage git worktrees                                                               |

## Environment Variables

| Variable           | Purpose                              | Defined In                                        |
| ------------------ | ------------------------------------ | ------------------------------------------------- |
| `ELPH_HOME`        | Override config/home directory       | `elph/src/platform/paths.rs`                      |
| `ELPH_DATA_DIR`    | Override data directory              | `elph/src/platform/paths.rs`, `cli/mod.rs`        |
| `ELPH_PROJECT_DIR` | Override project directory           | `elph/src/platform/paths.rs`                      |
| `ELPH_QUIET`       | Suppress init progress output        | `cli/mod.rs`, `bootstrap.rs`                      |
| `ELPH_PROVIDER`    | Default provider override            | `agent/provider.rs`                               |
| `ELPH_MODEL`       | Default model override               | `agent/provider.rs`                               |
| `ELPH_` prefix     | Agent env prefix for extended config | `cli/mod.rs` — `AgentBuilder::env_prefix("ELPH")` |

## Config Paths

From `elph/src/platform/paths.rs`:

| Path                            | Method                             | Description              |
| ------------------------------- | ---------------------------------- | ------------------------ |
| `~/.elph/`                      | `Paths::home_dir()`                | Home config directory    |
| `<project>/.elph/`              | `Paths::project_elph_dir()`        | Project-level config     |
| `<project>/.elph/settings.json` | `Paths::project_settings_path()`   | Project settings         |
| `~/.elph/settings.json`         | `Paths::home_settings_path()`      | Home settings (override) |
| `<project>/.elph/mcp.json`      | `Paths::project_mcp_config_path()` | MCP server config        |
| `<project>/.elph/store.db`      | `Paths::memory_db_path()`          | Floppy memory DB         |
| `<project>/.elph/plans/`        | `Paths::plans_dir()`               | Plan files               |
| `<data>/metadata.db`            | `Paths::metadata_db_path()`        | Metadata database        |
| `<data>/models/`                | `Paths::models_dir()`              | Model catalogs           |
| `~/.elph/extensions/`           | `Paths::global_extensions_dir()`   | Global extensions        |
| `<project>/.elph/extensions/`   | `Paths::project_extensions_dir()`  | Project extensions       |

## Makefile Targets

Key targets from `/Makefile`:

| Target                 | Description                                 |
| ---------------------- | ------------------------------------------- |
| `make build`           | Build elph binary (debug default)           |
| `make install`         | Build and install to `~/.local/bin/`        |
| `make run`             | Run elph coding agent                       |
| `make watch`           | Run with hot reload (watchexec)             |
| `make test`            | Run all workspace tests via `cargo nextest` |
| `make check`           | Check compilation (no codegen)              |
| `make lint`            | Run clippy with `-D warnings`               |
| `make fmt`             | Format all code                             |
| `make coverage`        | Run tests with coverage (cargo-llvm-cov)    |
| `make prepare`         | Install required toolchain tools            |
| `make generate-models` | Regenerate elph-ai model catalogs           |
| `make cross`           | Cross-compile for specific target           |
| `make release`         | Build release for host platform             |
| `make bump`            | Bump version (patch/minor/major)            |
| `make publish`         | Publish crates to crates.io                 |
| `make clean`           | Clean build artifacts                       |

## Observability

### Tracing

Enabled via the `tracing` feature flag on both `elph-agent` and `elph-ai`:

```toml
# elph-agent/Cargo.toml
tracing = ["dep:fastrace", "dep:fastrace-reqwest", "fastrace/enable", "elph-ai/tracing"]
```

Key spans (from `elph-agent/src/agent/harness/prompt_ops.rs`):

```
elph.agent.turn     — AgentHarness::prompt() (top-level turn span)
```

### Logging

`logforth` is the logging framework (configured in `elph/src/platform/bootstrap.rs`):

```toml
# Cargo.toml workspace dependency
logforth = { version = "0.30.1", features = [
  "append-async", "append-fastrace", "append-file", "diagnostic-fastrace",
  "filter-rustlog", "layout-json", "layout-text", "starter-log",
] }
```

Features: async appending, fastrace integration, JSON/text layout, file output.

### Agent Diagnostics

From `crates/elph-ai/src/utils/diagnostics.rs`:

- `create_assistant_message_diagnostic()` — creates diagnostic entries for assistant messages.
- `append_assistant_message_diagnostic()` — appends diagnostic to existing messages.

### Session Resource Cleanup

From `crates/elph-ai/src/session_resources.rs`:

- `register_session_resource_cleanup()` — register cleanup handlers.
- `cleanup_session_resources()` — runs all registered cleanup handlers.
- `SessionResourceCleanupRegistration` — handle for deregistration.

## Source References

- `elph/src/cli/mod.rs` — CLI subcommand definitions
- `elph/src/platform/paths.rs` — `Paths` struct, `PathResolver`, env var handling
- `elph/src/platform/settings.rs` — Settings loading/merging
- `elph/src/platform/bootstrap.rs` — logging initialization
- `Makefile` — build targets
- `crates/elph-ai/src/utils/diagnostics.rs` — diagnostic utilities
- `crates/elph-ai/src/session_resources.rs` — session resource cleanup
- `crates/elph-agent/src/agent/harness/prompt_ops.rs` — tracing spans
