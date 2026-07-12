---
title: "Quickstart Guide"
last_updated: 2026-07-30T10:00:00Z
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
| `elph-agent`                  | Agent runtime and tool execution engine (optional [TOON](prompt-encoding.md) prompt encoding for tool results).                                                                       |
| `elph-swarm`                  | Multi-agent swarm coordination.                                                                                                                                                       |
| `elph-tui`                    | Terminal UI components.                                                                                                                                                               |

> **Note:** This documentation focuses on the **owly** crate, which is the most recently developed component. For the main Elph CLI, see [`docs/`](../docs/).

---

## Owly: Agent Documentation Tool

Owly is a CLI that inspects a codebase and produces structured documentation under `openwiki/`. It uses `elph-agent` for the agent runtime and `elph-ai` for LLM provider integration.

### Quick Start

```sh
# Install from crates.io
cargo install --locked owly

# Or build from source
cargo install --path owly

# Initialize documentation (code mode — repository wiki)
owly code --init

# Initialize a personal knowledge wiki (~/.owly/wiki/)
owly personal --init

# Update existing documentation
owly code --update

# Ask a question in single-turn chat mode
owly "What does this project do?"

# Print response and exit
owly -p "Summarize the architecture"

# Stream LLM response live (no thinking display)
owly -s "What does this project do?"

# Stream with thinking display
owly -v "Explain the architecture"

# Product subcommands: auth, ingest, cron
owly auth list
owly ingest <path> --model "opencode/big-pickle"
owly cron list
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

### Code vs Personal Mode

Owly supports two run modes, set via positional argument or `--mode`:

| Mode       | Wiki root       | Use case                 |
| ---------- | --------------- | ------------------------ |
| `code`     | `./openwiki/`   | Repository documentation |
| `personal` | `~/.owly/wiki/` | Personal knowledge wiki  |

**Code mode** targets a codebase repository: the wiki lives under `openwiki/` in the repo, agent tools are rooted at the repo, and git history is used for change detection. **Personal mode** writes to `~/.owly/wiki/`, agent tools are rooted at the wiki itself, and there is no git dependency — the wiki is a flat knowledge base.

**Source:** [`owly/src/mode.rs`](../owly/src/mode.rs) — `RunMode` and `WikiContext` types.

### Command Reference

| Flag                | Description                                                                   |
| ------------------- | ----------------------------------------------------------------------------- |
| `--init`            | Generate initial documentation (requires mode positional: `owly code --init`) |
| `--update`          | Refresh existing documentation based on source changes                        |
| `--mode`            | Run mode: `code` or `personal` (or as positional arg)                         |
| `--print`, `-p`     | Run once and print output (non-interactive)                                   |
| `--stream`, `-s`    | Show streaming LLM response (without thinking)                                |
| `--model`           | Override model (e.g., `anthropic/claude-sonnet-5`)                            |
| `--verbose`, `-v`   | Show streaming response and thinking from LLM                                 |
| `--directory`, `-d` | Set working directory                                                         |
| `--dry-run`         | Plan only — no LLM run and no wiki writes                                     |
| `--credentials`     | Print credential diagnostics (keys masked) and exit                           |
| `--help`            | Show help                                                                     |

Product subcommands (`auth`, `ingest`, `cron`) are parsed from trailing arguments after the mode positional. See [`cli_product.rs`](../owly/src/cli_product.rs).

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

| File                                                                | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`owly/src/main.rs`](../owly/src/main.rs)                           | Entry point: initializes tracing, parses CLI, dispatches commands                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| [`owly/src/cli.rs`](../owly/src/cli.rs)                             | CLI argument definitions and `execute()` dispatch (mode resolution, product command routing)                                                                                                                                                                                                                                                                                                                                                                                                                              |
| [`owly/src/mode.rs`](../owly/src/mode.rs)                           | Run mode (`Code` vs `Personal`) and `WikiContext` (filesystem layout for wiki root, agent cwd, session anchor)                                                                                                                                                                                                                                                                                                                                                                                                            |
| [`owly/src/commands/mod.rs`](../owly/src/commands/mod.rs)           | Command dispatch: resolves config, validates input, delegates to [`doc_run.rs`](../owly/src/commands/doc_run.rs) (shared init/update agent runs) or [`non_interactive.rs`](../owly/src/commands/non_interactive.rs) (one-shot).                                                                                                                                                                                                                                                                                           |
| [`owly/src/commands/doc_run.rs`](../owly/src/commands/doc_run.rs)   | Shared init/update agent runs for both code and personal modes: snapshot creation, prompt building, agent execution, result handling.                                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/agent/mod.rs`](../owly/src/agent/mod.rs)                 | Agent integration: tool setup, prompt preparation, run loop. Sub-modules: [`commands.rs`](../owly/src/agent/commands.rs) (mode-aware prompt helpers), [`listeners.rs`](../owly/src/agent/listeners.rs) (event subscriptions with indicatif spinner), [`model.rs`](../owly/src/agent/model.rs) (model/auth resolution), [`run.rs`](../owly/src/agent/run.rs) (agent execution), [`tools.rs`](../owly/src/agent/tools.rs) (tool setup), [`shared_models.rs`](../owly/src/agent/shared_models.rs) (shared credential store). |
| [`owly/src/ask_user/`](../owly/src/ask_user/mod.rs)                 | Interactive tools: `ask_text`, `ask_select`, `ask_confirm` (dialoguer-based bridge, no TUI). Sub-modules: [`bridge.rs`](../owly/src/ask_user/bridge.rs), [`parse.rs`](../owly/src/ask_user/parse.rs).                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/checkpoint/mod.rs`](../owly/src/checkpoint/mod.rs)       | Conversation checkpointing (`TursoCheckpointSaver`). Sub-modules: [`saver/`](../owly/src/checkpoint/saver/mod.rs) (read/write/migrate), [`types.rs`](../owly/src/checkpoint/types.rs) (checkpoint data structures), [`util.rs`](../owly/src/checkpoint/util.rs) (helpers).                                                                                                                                                                                                                                                |
| [`owly/src/credentials/`](../owly/src/credentials/mod.rs)           | `~/.owly/.env` loading and API key management. Sub-modules: [`auth_context.rs`](../owly/src/credentials/auth_context.rs) (auth context), [`oauth_callbacks.rs`](../owly/src/credentials/oauth_callbacks.rs) (OAuth login flow), [`oauth_store.rs`](../owly/src/credentials/oauth_store.rs) (OAuth credential persistence).                                                                                                                                                                                                |
| [`owly/src/auth/`](../owly/src/auth/mod.rs)                         | OAuth provider configuration and auth subcommand (`owly auth configure` / `owly auth list`). Sub-modules: [`configure.rs`](../owly/src/auth/configure.rs), [`providers.rs`](../owly/src/auth/providers.rs).                                                                                                                                                                                                                                                                                                               |
| [`owly/src/cli_product.rs`](../owly/src/cli_product.rs)             | Product subcommand parsing: `auth`, `ingest`, `cron` — dispatched from trailing CLI args after mode.                                                                                                                                                                                                                                                                                                                                                                                                                      |
| [`owly/src/code_mode.rs`](../owly/src/code_mode.rs)                 | Code-mode repository setup: agent guidance snippets for `AGENTS.md`/`CLAUDE.md`, optional GitHub Actions workflow for scheduled `owly --update`.                                                                                                                                                                                                                                                                                                                                                                          |
| [`owly/src/connectors/`](../owly/src/connectors/mod.rs)             | External data connectors: `git_repo`, `hackernews`, `io`, `registry`, `web_search`, `x_source`.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| [`owly/src/ecosystem.rs`](../owly/src/ecosystem.rs)                 | Thin re-export of [`code_mode`](../owly/src/code_mode.rs) module for `AGENTS.md`/`CLAUDE.md` sync.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| [`owly/src/interactive.rs`](../owly/src/interactive.rs)             | Terminal user feedback: dialoguer prompts and indicatif progress spinner. `ensure_provider_setup()` runs the first-run credential wizard when needed.                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/onboarding.rs`](../owly/src/onboarding.rs)               | First-run credential onboarding wizard (provider selection, API key, OAuth, base URL, model).                                                                                                                                                                                                                                                                                                                                                                                                                             |
| [`owly/src/onboarding_config.rs`](../owly/src/onboarding_config.rs) | Personal wiki first-run flow (choose source connector, configure).                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| [`owly/src/prompts.rs`](../owly/src/prompts.rs)                     | System and user prompts for the agent — now includes personal-mode prompts alongside code-mode prompts.                                                                                                                                                                                                                                                                                                                                                                                                                   |
| [`owly/src/instructions.rs`](../owly/src/instructions.rs)           | Wiki brief management (`openwiki/INSTRUCTIONS.md`): read, save, prompt user when missing.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| [`owly/src/session/mod.rs`](../owly/src/session/mod.rs)             | Turso-backed session store with thread identity, message persistence, and crash recovery. Sub-modules: [`load.rs`](../owly/src/session/load.rs), [`persist.rs`](../owly/src/session/persist.rs), [`store.rs`](../owly/src/session/store.rs), [`thread.rs`](../owly/src/session/thread.rs), [`turn_write.rs`](../owly/src/session/turn_write.rs), [`types.rs`](../owly/src/session/types.rs).                                                                                                                              |
| [`owly/src/startup.rs`](../owly/src/startup.rs)                     | Non-interactive startup validation (credential checks, piped input checks).                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| [`owly/src/config.rs`](../owly/src/config.rs)                       | Provider/model resolution, config file loading                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| [`owly/src/constants/mod.rs`](../owly/src/constants/mod.rs)         | Provider definitions, default values, env var keys. Sub-modules: [`providers.rs`](../owly/src/constants/providers.rs) (provider metadata, auth methods), [`resolve.rs`](../owly/src/constants/resolve.rs) (auto-detection, validation).                                                                                                                                                                                                                                                                                   |
| [`owly/src/env.rs`](../owly/src/env.rs)                             | Environment validation and debug info                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/docs.rs`](../owly/src/docs.rs)                           | Documentation file read/write, snapshots, git summaries                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| [`owly/src/metadata.rs`](../owly/src/metadata.rs)                   | Update metadata tracking, git HEAD detection, no-op checks                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs)             | YAML frontmatter parsing and generation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs)             | Error sanitization (secret redaction), provider error handling                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| [`owly/src/ingestion.rs`](../owly/src/ingestion.rs)                 | File ingestion pipeline — reads and chunks files for personal wiki indexing.                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| [`owly/src/schedules.rs`](../owly/src/schedules.rs)                 | Cron/timer management for scheduled wiki updates (`owly cron`).                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| [`owly/src/help_content.rs`](../owly/src/help_content.rs)           | Extended help text displayed by `--help`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| [`owly/src/utils.rs`](../owly/src/utils.rs)                         | HTML tag stripping utility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| [`owly/src/lib.rs`](../owly/src/lib.rs)                             | Crate root — re-exports all public modules                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

### Tests

Integration and unit tests live in [`owly/tests/`](../owly/tests/):

| Test File                                                          | Tests                                                                                                |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| [`agent_test.rs`](../owly/tests/agent_test.rs)                     | Agent command preparation (`prepare_init_command`, `prepare_update_command`, `prepare_chat_command`) |
| [`checkpoint_test.rs`](../owly/tests/checkpoint_test.rs)           | Turso checkpoint saver integration tests                                                             |
| [`session_name_test.rs`](../owly/tests/session_name_test.rs)       | Per-thread display name metadata (`thread_metadata` table)                                           |
| [`config_test.rs`](../owly/tests/config_test.rs)                   | Config resolution, provider overrides, model ID handling                                             |
| [`constants_test.rs`](../owly/tests/constants_test.rs)             | Provider definitions and auto-detection logic                                                        |
| [`credentials_test.rs`](../owly/tests/credentials_test.rs)         | `.env` file loading and credential management                                                        |
| [`docs_test.rs`](../owly/tests/docs_test.rs)                       | Documentation file management                                                                        |
| [`env_ext_test.rs`](../owly/tests/env_ext_test.rs)                 | Environment variable handling                                                                        |
| [`env_test.rs`](../owly/tests/env_test.rs)                         | Environment setup                                                                                    |
| [`frontmatter_ext_test.rs`](../owly/tests/frontmatter_ext_test.rs) | Frontmatter parsing edge cases                                                                       |
| [`fs_errors_test.rs`](../owly/tests/fs_errors_test.rs)             | Filesystem error handling and edge cases                                                             |
| [`metadata_ext_test.rs`](../owly/tests/metadata_ext_test.rs)       | Update metadata, git summary, no-op detection                                                        |
| [`metadata_test.rs`](../owly/tests/metadata_test.rs)               | Metadata file read/write and format validation                                                       |
| [`onboarding_test.rs`](../owly/tests/onboarding_test.rs)           | First-run credential wizard tests                                                                    |
| [`personal_mode_test.rs`](../owly/tests/personal_mode_test.rs)     | Personal wiki mode tests                                                                             |
| [`prompts_test.rs`](../owly/tests/prompts_test.rs)                 | Prompt template generation                                                                           |
| [`redaction_ext_test.rs`](../owly/tests/redaction_ext_test.rs)     | Secret redaction patterns                                                                            |
| [`redaction_test.rs`](../owly/tests/redaction_test.rs)             | In-source redaction and diagnostics tests                                                            |
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

- **Run modes**: Owly supports both `code` (repository `openwiki/`) and `personal` (`~/.owly/wiki/`) modes via [`mode.rs`](../owly/src/mode.rs). The `WikiContext` type carries the resolved filesystem layout throughout the run.
- **Agent runtime**: Uses `elph-agent` (not LangChain/LangGraph). Agent loop and tool execution are delegated to `elph-agent`. Optional TOON encoding for structured tool results via `ELPH_PROMPT_ENCODING` — see [prompt-encoding.md](prompt-encoding.md).
- **LLM integration**: Uses `elph-ai` for provider abstraction. Model lookup goes through `shared_models()` in [`agent/shared_models.rs`](../owly/src/agent/shared_models.rs).
- **Tools**: Init/update mode uses all tools (read, bash, edit, write, grep, find, ls). Chat mode uses read-only tools plus `ask_text`, `ask_select`, `ask_confirm` for interactive use (via dialoguer, not TUI).
- **Product subcommands**: `auth` (OAuth configuration), `ingest` (file ingestion for personal wiki), `cron` (scheduled updates) are routed through [`cli_product.rs`](../owly/src/cli_product.rs).
- **Interactive feedback**: Terminal feedback uses dialoguer prompts and indicatif spinners via [`interactive.rs`](../owly/src/interactive.rs) — no TUI framework. Bare `owly` with no arguments prints "Interactive mode not yet implemented".
- **Session persistence**: Each owly run creates a `SessionStore` backed by Turso checkpointing. Conversation messages are persisted across turns and restorable on subsequent runs in the same directory. Mid-turn assistant drafts and pending `ask_*` interrupts are recovered from checkpoint `writes` on restart.
- **Auto session naming**: After the first chat turn completes, Owly calls `elph_agent::generate_session_name()` to generate a short title from user messages — stored in checkpoint `thread_metadata`. Used for display in terminal output.
- **Ecosystem sync**: After a successful init/update that changes documentation, [`code_mode.rs`](../owly/src/code_mode.rs) appends Owly context instructions to `AGENTS.md` and `CLAUDE.md` (if they exist) via [`ecosystem.rs`](../owly/src/ecosystem.rs) (thin re-export).
- **No-op detection**: The update command checks git HEAD and status to skip if nothing changed since the last documented update.
- **Runtime note**: A `create_runtime_note()` prompt is appended to all user prompts, telling the agent the repository root path and runtime conventions (relative paths only, no host absolute paths).
- **Secrets**: API keys are never written into documentation. The diagnostics module redacts credentials from error output. The `~/.owly/` directory is secured with `0o700` permissions on Unix.

---

## Next Steps

- [Architecture](architecture.md) — Deep dive into module structure and agent execution flow
- [Configuration](configuration.md) — Supported providers, model selection, environment setup
- [TOON prompt encoding](prompt-encoding.md) — Optional compression for structured tool results (`ELPH_PROMPT_ENCODING`)
- [Elph design docs](../docs/) — product specs (behavior, UX, architecture); implementation detail stays in openwiki
