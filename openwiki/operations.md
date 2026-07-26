---
type: Guide
title: Operations and Configuration
description: CLI subcommands, configuration system, environment variables, directory layout, observability, and day-to-day operations.
tags: [operations, cli, config, observability, runbook]
resource: /elph/
---

# Operations and Configuration

## CLI subcommands

The `elph` binary exposes 17+ subcommands via `clap` ([source](../architecture/source-map.md#elph-binary--library-crate--elph)).

### Interactive TUI

```sh
elph                    # Launch interactive TUI
elph --resume <id>      # Resume a specific session
elph --version          # Print version
```

### Non-interactive run

```sh
elph run "Write a Rust function to parse CSV"  # Single-turn agent execution
elph run --provider anthropic --model claude-sonnet-4-20250514 "query"
elph run --no-mcp "query"                       # Skip MCP discovery
```

### Session management

```sh
elph session list       # List all sessions
elph session export <id>  # Export session transcript
elph session delete <id>  # Delete a session
```

### Memory

```sh
elph memory status           # Show memory store status
elph memory search "query"   # Semantic memory search
elph memory tasks            # List memory tasks
```

### MCP

See [mcp-integration.md](mcp-integration.md) for full reference.

```sh
elph mcp list                              # List configured servers
elph mcp add <name> '<config_json>'        # Add a server
elph mcp remove <name>                     # Remove a server
elph mcp doctor                            # Validate configurations
elph mcp listen                            # Listen for MCP notifications
```

### Admin

```sh
elph doctor              # Show discovered configuration
elph models list         # Browse available models
elph models catalog      # Generate model catalog
elph provider list       # List configured providers
elph stats               # Show usage statistics
elph export              # Export sessions
elph import              # Import sessions
elph update              # Self-update
elph completions bash    # Generate shell completions
```

### Slash commands

| Command    | Description                                                             |
| ---------- | ----------------------------------------------------------------------- |
| `/new`     | Start a fresh session in-place (reloads resources, no exit + re-launch) |
| `/compact` | Compact conversation history                                            |
| `/goal`    | Manage goals/todos                                                      |
| `/reload`  | Reload extensions and skills                                            |

**Source:** `/elph/src/agent/slash_commands.rs`, `/elph/src/tui/slash_handler.rs`

### BackgroundTask async dispatch

Slash commands that require an agent session but should not block the TUI (e.g. `/goal`, `/reload`, `/extension`) can be dispatched as background tasks via `SlashOutcome::BackgroundTask`. The TUI continues to accept input while the task runs in the background.

**Source:** `/elph/src/tui/slash_handler.rs` — `SlashOutcome::BackgroundTask`

### Shell execution timeout

The `elph-exec` shell execution layer uses deadline-based timeouts (`tokio::time::sleep_until`) instead of interval-based sleeps, and caps streaming tool output at 100KB to prevent rendering slowdowns from multi-MB outputs.

**Source:** `/crates/elph-exec/src/shell.rs`

**Source:** `/elph/src/cli/mod.rs` and submodule files.

## Configuration system

### Directory layout

```
~/.elph/                                    # XDG_CONFIG_HOME
├── settings.json          # UI and session preferences
├── mcp.json               # Global MCP server configs
├── auth.json              # Encrypted OAuth tokens
├── providers/
│   ├── openai.json
│   ├── anthropic.json
│   └── ...                # One JSON file per provider
├── prompts/               # Global prompt templates
├── extensions/            # WASM extension bundles
└── skills/                # Global skills

~/.local/share/elph/       # XDG_DATA_HOME
├── version.json
├── metadata.db            # SQLite/Turso platform sessions
├── attachments/           # Pasted images per session
├── models/                # Embedding model cache
└── logs/
    ├── elph.jsonl         # Structured log output
    └── elph-traces.jsonl  # fastrace trace spans

<workDir>/.agents/         # Shared agent prompts and skills (gitignored)
├── prompts/*.md
└── skills/<name>/SKILL.md

<workDir>/.elph/           # Project-local configuration (gitignored)
├── settings.json          # Optional project settings overrides
├── mcp.json               # Per-project MCP config
├── store.db               # Agent memory (floppy)
├── prompts/*.md
├── extensions/
├── skills/
└── metadata/              # Session metadata and logs
```

**Source:** `/elph/src/platform/paths.rs`

### Layered settings

Merge order: **Defaults** → **Home** (`~/.elph/settings.json`) → **Project** (`<workDir>/.elph/settings.json`)

- Project overrides per **nested key** (deep merge)
- Runtime saves write **home only**, never project
- Settings schema: `/schemas/elph-schema.json`

**Source:** `/elph/src/platform/settings.rs`

### Settings domains

Settings are JSON with domain groups:

| Domain     | Key settings                                           |
| ---------- | ------------------------------------------------------ |
| `ui`       | Theme (auto/dark/light), thinking level, scoped models |
| `session`  | Auto-save interval, max turns, compaction limits       |
| `models`   | Model preferences, provider mapping                    |
| `provider` | Default provider, API configuration                    |
| `memory`   | Floppy memory store connection                         |

### Environment variables

| Variable                      | Effect                                               |
| ----------------------------- | ---------------------------------------------------- |
| `ELPH_HOME`                   | Override `~/.elph` config dir                        |
| `ELPH_DATA_DIR`               | Override data directory                              |
| `ELPH_PROJECT_DIR`            | Project root for `.elph/`                            |
| `ELPH_PROVIDER`               | Force provider ID                                    |
| `ELPH_MODEL`                  | Force model ID                                       |
| `ELPH_PROMPT_ENCODING`        | Tool-result encoding: `off`, `toon`, `auto`          |
| `ELPH_TRACE`                  | Enable/disable distributed tracing (`fastrace`)      |
| `ELPH_LOG_LEVEL`              | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `ELPH_LOG_FILE`               | Log file (set `0` to disable)                        |
| `ELPH_LOG_ROTATION`           | Rotation: `hourly`, `daily`, `weekly`                |
| `ELPH_QUIET`                  | Suppress bootstrap output                            |
| `ELPH_MCP_DISABLED`           | Skip MCP discovery                                   |
| `ELPH_MCP_FETCH_TIMEOUT_SECS` | Per-server connection timeout                        |

Provider keys are read from standard env vars: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, etc.

**Source:** `/docs/configuration.md`, `/elph/src/platform/paths.rs`

## Observability

### Logging

Uses `log` + `logforth` for structured JSONL logging.

| Output | Path                           | Control                                                |
| ------ | ------------------------------ | ------------------------------------------------------ |
| Logs   | `{logs_dir}/elph.jsonl`        | `ELPH_LOG_LEVEL`, `ELPH_LOG_FILE`, `ELPH_LOG_ROTATION` |
| Traces | `{logs_dir}/elph-traces.jsonl` | `ELPH_TRACE` (set `0` to disable)                      |

**Source:** `crates/elph-core/src/logger/`, `crates/elph-agent/src/trace/`

### Distributed tracing

- `fastrace` spans for agent loop, tool execution, LLM calls
- HTTP `traceparent` header propagation for downstream tracing
- Feature-gated behind `tracing` Cargo feature (enabled by default in `elph` binary)

**Source:** `crates/elph-core/src/trace/`, `crates/elph-agent/src/trace/`, documentation at `crates/elph-agent/docs/observability.md`

## Make targets

Key targets from the root `Makefile`:

| Target         | Description                            |
| -------------- | -------------------------------------- |
| `make check`   | `cargo check --workspace`              |
| `make build`   | Build `elph` binary                    |
| `make test`    | `cargo nextest run`                    |
| `make lint`    | `cargo clippy --workspace -D warnings` |
| `make fmt`     | `cargo fmt` (edition 2024)             |
| `make run`     | `cargo run --bin elph`                 |
| `make install` | Copy binary to `~/.local/bin`          |
| `make clean`   | Clean build artifacts                  |
| `make stats`   | Code statistics (cloc)                 |
| `make prepare` | Install toolchain, tools, vendor deps  |
| `make publish` | Publish crates to crates.io            |
| `make release` | Multi-arch release builds              |

**Source:** `/Makefile`

## WASM extensions

Phase 1 extension support via wasmtime Component Model:

- **Discovery**: `~/.elph/extensions/` and `<project>/.elph/extensions/`
- **Slash commands**: extensions can register slash commands
- **Development**: build guest WASM, install with `elph plugin install`

**Source:** `/elph/src/extensions/`, `/docs/extensions.md`

## Troubleshooting

```sh
# Check discovered config
elph doctor

# Validate MCP setup
elph mcp doctor

# View logs
tail -f ~/.local/share/elph/logs/elph.jsonl

# Run with verbose logging
ELPH_LOG_LEVEL=debug elph

# Disable MCP for troubleshooting
elph run --no-mcp "query"
```
