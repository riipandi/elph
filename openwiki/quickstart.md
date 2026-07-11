---
title: "Quickstart Guide"
last_updated: 2026-07-28T10:00:00Z
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

# Initialize documentation
owly --init

# Update existing documentation
owly --update

# Start interactive chat (multi-turn, with ask tools)
owly

# Ask a question in single-turn chat mode
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

| File                                                                        | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`owly/src/main.rs`](../owly/src/main.rs)                                   | Entry point: initializes tracing, parses CLI, dispatches commands                                                                                                                                                                                                                                                                                                                                                                               |
| [`owly/src/cli.rs`](../owly/src/cli.rs)                                     | CLI argument definitions and `execute()` dispatch                                                                                                                                                                                                                                                                                                                                                                                               |
| [`owly/src/commands/mod.rs`](../owly/src/commands/mod.rs)                   | Command dispatch: `Init`, `Update`, `Chat` variants. Sub-module [`non_interactive.rs`](../owly/src/commands/non_interactive.rs) handles one-shot execution.                                                                                                                                                                                                                                                                                     |
| [`owly/src/agent/mod.rs`](../owly/src/agent/mod.rs)                         | Agent integration: tool setup, prompt preparation, run loop. Sub-modules: [`commands.rs`](../owly/src/agent/commands.rs) (prompt helpers), [`listeners.rs`](../owly/src/agent/listeners.rs) (event subscriptions), [`model.rs`](../owly/src/agent/model.rs) (model resolution), [`run.rs`](../owly/src/agent/run.rs) (agent execution), [`tools.rs`](../owly/src/agent/tools.rs) (read-only / full tool setup).                                 |
| [`owly/src/ask_user/`](../owly/src/ask_user/mod.rs)                         | Interactive tools: `ask_text`, `ask_select`, `ask_confirm` (bridge + TUI integration). Sub-modules: [`bridge.rs`](../owly/src/ask_user/bridge.rs), [`parse.rs`](../owly/src/ask_user/parse.rs).                                                                                                                                                                                                                                                 |
| [`owly/src/checkpoint/mod.rs`](../owly/src/checkpoint/mod.rs)               | Conversation checkpointing (`TursoCheckpointSaver`). Sub-modules: [`saver/`](../owly/src/checkpoint/saver/mod.rs) (read/write/migrate), [`types.rs`](../owly/src/checkpoint/types.rs) (checkpoint data structures), [`util.rs`](../owly/src/checkpoint/util.rs) (helpers).                                                                                                                                                                      |
| [`owly/src/ecosystem.rs`](../owly/src/ecosystem.rs)                         | Repository ecosystem hooks (`AGENTS.md` / `CLAUDE.md` sync)                                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/onboarding.rs`](../owly/src/onboarding.rs)                       | First-run credential onboarding wizard                                                                                                                                                                                                                                                                                                                                                                                                          |
| [`owly/src/prompts.rs`](../owly/src/prompts.rs)                             | System and user prompts for the agent                                                                                                                                                                                                                                                                                                                                                                                                           |
| [`owly/src/session/mod.rs`](../owly/src/session/mod.rs)                     | Turso-backed session store with thread identity, message persistence, and crash recovery. Sub-modules: [`load.rs`](../owly/src/session/load.rs), [`persist.rs`](../owly/src/session/persist.rs), [`store.rs`](../owly/src/session/store.rs), [`thread.rs`](../owly/src/session/thread.rs), [`turn_write.rs`](../owly/src/session/turn_write.rs), [`types.rs`](../owly/src/session/types.rs).                                                    |
| [`owly/src/shell/mod.rs`](../owly/src/shell/mod.rs)                         | Interactive Owly shell — REPL dispatch, credential setup, initial command execution. Sub-modules: [`checkpoint_cmd.rs`](../owly/src/shell/checkpoint_cmd.rs), [`commands.rs`](../owly/src/shell/commands.rs), [`help.rs`](../owly/src/shell/help.rs), [`input.rs`](../owly/src/shell/input.rs), [`output.rs`](../owly/src/shell/output.rs), [`startup.rs`](../owly/src/shell/startup.rs), [`writer_util.rs`](../owly/src/shell/writer_util.rs). |
| [`owly/src/startup.rs`](../owly/src/startup.rs)                             | Startup mode resolution (non-interactive vs. interactive), TTY validation                                                                                                                                                                                                                                                                                                                                                                       |
| [`owly/src/config.rs`](../owly/src/config.rs)                               | Provider/model resolution, config file loading                                                                                                                                                                                                                                                                                                                                                                                                  |
| [`owly/src/constants/mod.rs`](../owly/src/constants/mod.rs)                 | Provider definitions, default values, env var keys. Sub-modules: [`providers.rs`](../owly/src/constants/providers.rs), [`resolve.rs`](../owly/src/constants/resolve.rs).                                                                                                                                                                                                                                                                        |
| [`owly/src/credentials.rs`](../owly/src/credentials.rs)                     | `~/.owly/.env` loading and API key management                                                                                                                                                                                                                                                                                                                                                                                                   |
| [`owly/src/env.rs`](../owly/src/env.rs)                                     | Environment validation and debug info                                                                                                                                                                                                                                                                                                                                                                                                           |
| [`owly/src/docs.rs`](../owly/src/docs.rs)                                   | Documentation file read/write, snapshots, git summaries                                                                                                                                                                                                                                                                                                                                                                                         |
| [`owly/src/metadata.rs`](../owly/src/metadata.rs)                           | Update metadata tracking, git HEAD detection, no-op checks                                                                                                                                                                                                                                                                                                                                                                                      |
| [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs)                     | YAML frontmatter parsing and generation                                                                                                                                                                                                                                                                                                                                                                                                         |
| [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs)                     | Error sanitization (secret redaction), provider error handling                                                                                                                                                                                                                                                                                                                                                                                  |
| [`owly/src/utils.rs`](../owly/src/utils.rs)                                 | HTML tag stripping utility                                                                                                                                                                                                                                                                                                                                                                                                                      |
| [`owly/src/ui_events.rs`](../owly/src/ui_events.rs)                         | Agent→TUI event bridge (streaming text, tool status, command progress)                                                                                                                                                                                                                                                                                                                                                                          |
| [`owly/src/tui/mod.rs`](../owly/src/tui/mod.rs)                             | [tuie](https://crates.io/crates/tuie) interactive shell entrypoint (`run_interactive()`)                                                                                                                                                                                                                                                                                                                                                        |
| [`owly/src/tui/app/mod.rs`](../owly/src/tui/app/mod.rs)                     | Owly interactive shell application (`OwlyApp` with `run_shell()`). Sub-modules: [`events.rs`](../owly/src/tui/app/events.rs), [`input.rs`](../owly/src/tui/app/input.rs), [`run.rs`](../owly/src/tui/app/run.rs), [`setup.rs`](../owly/src/tui/app/setup.rs).                                                                                                                                                                                   |
| [`owly/src/tui/shell_host.rs`](../owly/src/tui/shell_host.rs)               | `OwlyShellHost` — implements `elph-tui`'s `ShellHost` trait for the shared tuie agent shell                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/tui/transcript_render.rs`](../owly/src/tui/transcript_render.rs) | Formats `OwlyEntry` rows into scrollable transcript lines for the tuie pane                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/tui/setup_tuie.rs`](../owly/src/tui/setup_tuie.rs)               | First-run credential setup wizard (tuie widget)                                                                                                                                                                                                                                                                                                                                                                                                 |
| [`owly/src/tui/slash.rs`](../owly/src/tui/slash.rs)                         | Slash-command definitions and palette integration (`/` in prompt, `Ctrl+K` for full list)                                                                                                                                                                                                                                                                                                                                                       |
| [`owly/src/tui/entries.rs`](../owly/src/tui/entries.rs)                     | Typed transcript entries (`OwlyEntry`, `OwlyEntryKind`)                                                                                                                                                                                                                                                                                                                                                                                         |
| [`owly/src/tui/transcript/mod.rs`](../owly/src/tui/transcript/mod.rs)       | `TranscriptApplier`: maps `AgentUiEvent` → `OwlyEntry` list updates. Sub-modules: [`applier.rs`](../owly/src/tui/transcript/applier.rs), [`helpers.rs`](../owly/src/tui/transcript/helpers.rs).                                                                                                                                                                                                                                                 |
| [`owly/src/tui/tool_display.rs`](../owly/src/tui/tool_display.rs)           | Shared formatting for tool execution output (`tool_output_preview`, `tool_chip_label`, `tool_transcript_compact`, `tool_transcript_body`)                                                                                                                                                                                                                                                                                                       |
| [`owly/src/tui/context.rs`](../owly/src/tui/context.rs)                     | Thread-safe `AppContext` for TUI and async command dispatch                                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/tui/launch.rs`](../owly/src/tui/launch.rs)                       | One-shot launch payload for the Owly interactive shell                                                                                                                                                                                                                                                                                                                                                                                          |
| [`owly/src/tui/setup.rs`](../owly/src/tui/setup.rs)                         | In-TUI first-run credential setup wizard                                                                                                                                                                                                                                                                                                                                                                                                        |
| [`owly/src/tui/banner.rs`](../owly/src/tui/banner.rs)                       | Welcome banner as transcript hint entries (`directory_display` path helper)                                                                                                                                                                                                                                                                                                                                                                     |
| [`owly/src/lib.rs`](../owly/src/lib.rs)                                     | Crate root — re-exports all public modules                                                                                                                                                                                                                                                                                                                                                                                                      |

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

- **Agent runtime**: Uses `elph-agent` (not LangChain/LangGraph). Agent loop and tool execution are delegated to `elph-agent`. Optional TOON encoding for structured tool results via `ELPH_PROMPT_ENCODING` — see [prompt-encoding.md](prompt-encoding.md).
- **LLM integration**: Uses `elph-ai` for provider abstraction. Model lookup goes through `builtin_models()`.
- **Tools**: Init/update mode uses all tools (read, bash, edit, write, grep, find, ls). Chat mode uses read-only tools plus `ask_text`, `ask_select`, `ask_confirm` for interactive use.
- **Interactive mode**: Running `owly` with no arguments starts a [tuie](https://crates.io/crates/tuie)-based shell via [`tui/mod.rs`](../owly/src/tui/mod.rs) — session persistence ([`session/mod.rs`](../owly/src/session/mod.rs)), first-run setup (`setup_tuie.rs`), and follow-up commands after init/update/chat. Keybindings: `Enter` send / queue, `Ctrl+Enter` steer while busy, `Esc` clear prompt, `Tab` cycle mode, `Shift+↑/↓` scroll transcript, `Shift+End` jump tail, `Ctrl+K` command palette, `Ctrl+S` toggle sidebar (wide terminals), `Ctrl+C` cancel. Shared widgets and chords live in [`elph-tui`](../crates/elph-tui/).
- **Interactive slash commands**: `/init`, `/update`, `/history [n]`, `/restore <#|id>`, `/clear`, `/name`, `/help`, `/exit`. `/history` lists recent checkpoints; `/restore` rewinds the session to an earlier checkpoint (the next turn forks from that point). `/name` shows the current session title; `/name <title>` sets it manually.
- **Session persistence**: Each owly run creates a `SessionStore` backed by Turso checkpointing. Conversation messages are persisted across turns and restorable on subsequent runs in the same directory. Mid-turn assistant drafts and pending `ask_*` interrupts are recovered from checkpoint `writes` on restart.
- **Auto session naming**: After the first interactive **chat** turn completes, Owly calls the session model to generate a short title (max 60 characters) from user messages — inspired by [pi-auto-session-name](https://github.com/therynamo/pi-auto-session-name). The title is stored in the checkpoint DB (`thread_metadata` table) and shown in the TUI footer instead of the raw thread id. Naming runs once per thread; use `/name` to override. `/clear` starts a new thread and resets the title until the next auto-name.
- **Ecosystem sync**: After a successful init/update that changes documentation, [`ecosystem.rs`](../owly/src/ecosystem.rs) appends Owly context instructions to `AGENTS.md` and `CLAUDE.md` (if they exist).
- **No-op detection**: The update command checks git HEAD and status to skip if nothing changed since the last documented update.
- **Runtime note**: A `create_runtime_note()` prompt is appended to all user prompts, telling the agent the repository root path and runtime conventions (relative paths only, no host absolute paths).
- **Secrets**: API keys are never written into documentation. The diagnostics module redacts credentials from error output. The `~/.owly/` directory is secured with `0o700` permissions on Unix.

---

## Next Steps

- [Architecture](architecture.md) — Deep dive into module structure and agent execution flow
- [Configuration](configuration.md) — Supported providers, model selection, environment setup
- [TOON prompt encoding](prompt-encoding.md) — Optional compression for structured tool results (`ELPH_PROMPT_ENCODING`)
- [Elph design docs](../docs/) — product specs (behavior, UX, architecture); implementation detail stays in openwiki
