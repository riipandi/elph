---
title: "Quickstart Guide"
last_updated: 2026-07-08T14:14:00Z
category: quickstart
tags:
    - getting-started
    - overview
    - repository
status: published
---

# Quickstart Guide

## Repository Overview

**Elph** is a Rust workspace for building and deploying AI agent applications. The project provides several crates for agent runtime, LLM integration, and tooling.

### Workspace Crates

| Crate                         | Description                                                                                                                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`owly`](../owly/README.md)   | CLI tool that writes and maintains documentation for codebases using AI agents (port of [OpenWiki](https://github.com/langchain-ai/openwiki)). Source at [`owly/src/`](../owly/src/). |
| [`elph`](../elph/README.md)   | Main CLI binary for the Elph agent platform.                                                                                                                                          |
| [`eclaw`](../eclaw/README.md) | Cross-compilation and release tooling.                                                                                                                                                |
| `elph-core`                   | Core library for agent data models and runtime primitives.                                                                                                                            |
| `elph-ai`                     | LLM provider integration layer.                                                                                                                                                       |
| `elph-agent`                  | Agent runtime and tool execution engine.                                                                                                                                              |
| `elph-swarm`                  | Multi-agent swarm coordination.                                                                                                                                                       |
| `elph-tui`                    | Terminal UI components.                                                                                                                                                               |

> **Note:** This documentation focuses on the **owly** crate, which is the most recently developed component. For the main Elph CLI, see [`docs/`](../docs/).

---

## Owly: Agent Documentation Tool

Owly is a CLI that inspects a codebase and produces structured documentation under `openwiki/`. It uses `elph-agent` for the agent runtime and `elph-ai` for LLM provider integration.

### Quick Start

```sh
# Install from source
cargo install --path owly

# Initialize documentation
owly --init

# Update existing documentation
owly --update

# Ask a question in chat mode
owly "What does this project do?"

# Print response and exit
owly -p "Summarize the architecture"

# Stream LLM response live (no thinking display)
owly -s "What does this project do?"

# Stream with thinking display
owly -v "Explain the architecture"
```

### Required Setup

Owly needs an API key for an LLM provider. Set it in your environment:

```sh
export OPENCODE_API_KEY="your-key-here"
# or any supported provider key (see configuration)
```

Or create a `~/.owly/.env` file:

```env
OWLY_PROVIDER=opencode
OWLY_MODEL_ID=big-pickle
OPENCODE_API_KEY=your-api-key-here
```

### Command Reference

| Flag                | Description                                            |
| ------------------- | ------------------------------------------------------ |
| `--init`            | Generate initial documentation under `openwiki/`       |
| `--update`          | Refresh existing documentation based on source changes |
| `--print`, `-p`     | Run once and print output (non-interactive)            |
| `--stream`, `-s`    | Show streaming LLM response (without thinking)         |
| `--model`           | Override model (e.g., `anthropic/claude-sonnet-5`)     |
| `--verbose`, `-v`   | Show streaming response and thinking from LLM          |
| `--directory`, `-d` | Set working directory                                  |
| `--help`            | Show help                                              |

Source: [`owly/src/cli.rs`](../owly/src/cli.rs) — CLI argument parsing and command dispatch.

### Documentation Structure

Owly writes to the `openwiki/` directory:

```
openwiki/
├── quickstart.md         # Entry point (this file)
├── .last-update.json     # Update metadata (git HEAD, timestamp, model)
├── architecture/         # Architecture documentation
├── workflows/            # Workflow documentation
├── domain/               # Domain-specific documentation
├── api/                  # API documentation
├── operations/           # Operations documentation
├── integrations/         # Integration documentation
└── testing/              # Testing documentation
```

Every Markdown file includes [YAML frontmatter](frontmatter.md) with title, last_updated, category, tags, and status.

---

## Key Source Files (owly crate)

| File                                                    | Purpose                                                           |
| ------------------------------------------------------- | ----------------------------------------------------------------- |
| [`owly/src/main.rs`](../owly/src/main.rs)               | Entry point: initializes tracing, parses CLI, dispatches commands |
| [`owly/src/cli.rs`](../owly/src/cli.rs)                 | CLI argument definitions and `execute()` dispatch                 |
| [`owly/src/commands.rs`](../owly/src/commands.rs)       | Command implementations: `init`, `update`, `chat`                 |
| [`owly/src/agent.rs`](../owly/src/agent.rs)             | Agent integration: tool setup, prompt preparation, run loop       |
| [`owly/src/prompts.rs`](../owly/src/prompts.rs)         | System and user prompts for the agent                             |
| [`owly/src/config.rs`](../owly/src/config.rs)           | Provider/model resolution, config file loading                    |
| [`owly/src/constants.rs`](../owly/src/constants.rs)     | Provider definitions, default values, env var keys                |
| [`owly/src/credentials.rs`](../owly/src/credentials.rs) | `~/.owly/.env` loading and API key management                     |
| [`owly/src/env.rs`](../owly/src/env.rs)                 | Environment validation and debug info                             |
| [`owly/src/docs.rs`](../owly/src/docs.rs)               | Documentation file read/write, snapshots, git summaries           |
| [`owly/src/metadata.rs`](../owly/src/metadata.rs)       | Update metadata tracking, git HEAD detection, no-op checks        |
| [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs) | YAML frontmatter parsing and generation                           |
| [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs) | Error sanitization (secret redaction), provider error handling    |
| [`owly/src/utils.rs`](../owly/src/utils.rs)             | HTML tag stripping utility                                        |
| [`owly/src/lib.rs`](../owly/src/lib.rs)                 | Crate root — re-exports all public modules                        |

### Tests

Integration and unit tests live in [`owly/tests/`](../owly/tests/):

| Test File                                                          | Tests                                                                                                |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| [`agent_test.rs`](../owly/tests/agent_test.rs)                     | Agent command preparation (`prepare_init_command`, `prepare_update_command`, `prepare_chat_command`) |
| [`config_test.rs`](../owly/tests/config_test.rs)                   | Config resolution, provider overrides, model ID handling                                             |
| [`docs_test.rs`](../owly/tests/docs_test.rs)                       | Documentation file management                                                                        |
| [`frontmatter_ext_test.rs`](../owly/tests/frontmatter_ext_test.rs) | Frontmatter parsing edge cases                                                                       |
| [`metadata_ext_test.rs`](../owly/tests/metadata_ext_test.rs)       | Update metadata, git summary, no-op detection                                                        |
| [`prompts_test.rs`](../owly/tests/prompts_test.rs)                 | Prompt template generation                                                                           |
| [`redaction_ext_test.rs`](../owly/tests/redaction_ext_test.rs)     | Secret redaction patterns                                                                            |
| [`env_ext_test.rs`](../owly/tests/env_ext_test.rs)                 | Environment variable handling                                                                        |
| [`env_test.rs`](../owly/tests/env_test.rs)                         | Environment setup                                                                                    |
| [`utils_test.rs`](../owly/tests/utils_test.rs)                     | HTML stripping utility                                                                               |

---

## Development

### Build

```sh
cargo build -p owly
```

### Test

```sh
cargo test -p owly
```

### Lint

```sh
cargo clippy -p owly --all-targets -- -D warnings
```

### Key Development Notes

- **Agent runtime**: Uses `elph-agent` (not LangChain/LangGraph). Agent loop and tool execution are delegated to `elph-agent`.
- **LLM integration**: Uses `elph-ai` for provider abstraction. Model lookup goes through `builtin_models()`.
- **Tools**: Init/update mode uses all tools (read, bash, edit, write, grep, find, ls). Chat mode uses read-only tools (read, grep, find, ls).
- **No-op detection**: The update command checks git HEAD and status to skip if nothing changed since the last documented update.
- **Secrets**: API keys are never written into documentation. The diagnostics module redacts credentials from error output.

---

## Next Steps

- [Architecture](architecture.md) — Deep dive into module structure and agent execution flow
- [Configuration](configuration.md) — Supported providers, model selection, environment setup
- [Existing Elph docs](../docs/) — CLI usage, memory, TUI, and operational considerations
