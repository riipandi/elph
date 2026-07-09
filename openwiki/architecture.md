---
title: "Architecture"
last_updated: 2026-07-10T20:00:00Z
category: architecture
tags:
    - architecture
    - design
    - modules
status: published
---

# Architecture

## Overview

Owly is a CLI agent that generates and maintains codebase documentation. It follows a pipeline: **CLI → Command → Agent → LLM → Filesystem**.

```
User Input
    │
    ├── --print or piped stdin ──────────────────────────────────────────────┐
    │                                                                        │
    ▼                                                                        ▼
┌──────────┐    ┌───────────┐    ┌────────────────────┐    ┌──────────────────────────┐
│  cli.rs  │───▶│commands.rs│───▶│  startup.rs        │───▶│    shell.rs              │
│ (parsing)│    │(dispatch) │    │ (mode resolution)  │    │ (interactive REPL)       │
└──────────┘    └───────────┘    └────────────────────┘    └──────────────────────────┘
                      │                    │                          │
                      │         ┌──────────┴──────────┐               │
                      │         ▼                     ▼               │
                      │  ┌──────────────────┐  ┌──────────────────┐   │
                      │  │ agent.rs         │  │ session.rs       │   │
                      │  │ (prompt + run)   │  │ (checkpoint)     │   │
                      │  └──────────────────┘  └──────────────────┘   │
                      │         │                                     │
                      │         ▼                                     │
                      │  ┌──────────────────────────┐                 │
                      │  │ elph-agent + elph-ai     │                 │
                      │  │ (tools, LLM, streaming)  │                 │
                      │  └──────────────────────────┘                 │
                      │         │                                     │
                      ▼         ▼                                     │
                  ┌────────────────────┐                              │
                  │    Filesystem      │◀─────────────────────────────┘
                  │  (openwiki/ docs)  │
                  └────────────────────┘
```

---

## Module Architecture

### 1. Entrypoint — [`main.rs`](../owly/src/main.rs)

Initializes `tracing` logging, parses CLI arguments via `clap`, and calls `cli.execute()`.

### 2. CLI Layer — [`cli.rs`](../owly/src/cli.rs)

Defines the `Cli` struct with clap derive macros. Supported flags:

- `--init` / `--update` — select mode
- `--model` — override provider/model
- `--print` / `--stream` / `--verbose` — output control (`--stream` shows text deltas, `--verbose` adds thinking in dimmed gray)
- `--directory` — working directory
- Trailing argument — chat message

The `execute()` method resolves the command enum and calls `run_command()`, forwarding the `stream` flag. When no arguments are provided, the CLI now dispatches to interactive chat (`Command::Chat { message: None }`) instead of exiting with an error — see [Interactive Mode](#interactive-mode) below.

**Banner output** uses ANSI color codes (cyan for logo, green for values, dimmed for labels).

**Source:** [`owly/src/cli.rs`](../owly/src/cli.rs) — ported from OpenWiki `src/cli.tsx`.

### 3. Command Dispatch — [`commands.rs`](../owly/src/commands.rs)

The `run_command()` function now delegates to [`startup.rs`](../owly/src/startup.rs) to resolve the startup mode:

- **`StartupMode::NonInteractive`** — used when `--print` is set or stdin is piped. Validates credentials and input via `validate_non_interactive()`, sets up the environment, and calls `run_non_interactive()`.
- **`StartupMode::Interactive`** — default when running in a terminal. Opens the interactive shell via `shell::run()`, which may perform an initial command (init/update/chat) and then enters a REPL for follow-up messages.

`run_non_interactive()` creates a `SessionStore` and dispatches to mode-specific functions:

| Command            | Behavior                                                                                                         |
| ------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `Init`             | Checks if `openwiki/` exists. If yes, delegates to update path. If no, runs agent with init prompt.              |
| `Update`           | Checks if `openwiki/` exists. If no, delegates to init path. Checks no-op status. Runs agent with update prompt. |
| `Chat { message }` | Single-turn chat with read-only tools. Interactive chat requires a terminal.                                     |

Each non-interactive command:

1. Loads credentials from `~/.owly/.env`
2. Resolves configuration (provider, model)
3. Creates a `SessionStore` (Turso checkpoint)
4. Takes a documentation snapshot before running
5. Prepares system + user prompts (runtime note appended)
6. Runs the agent with session and snapshot
7. Saves update metadata only when docs actually changed (`save_update_metadata_if_changed`)
8. Syncs ecosystem hooks (`ecosystem::sync_agent_guidance_files`) on docs change

#### Interactive Mode

When `owly` is run with no arguments (or in a TTY), the startup mode resolves to `Interactive`:

1. Optionally runs a first-time credential wizard (`onboarding::run_wizard()`) if no API key is found
2. Sets up environment and prints the welcome banner
3. Opens a `SessionStore` (restoring previous session messages if available)
4. Checks for crash recovery via `SessionRecovery`: restored partial assistant drafts and pending `ask_*` interrupts are reported as startup hints
5. Runs the initial command (init/update/chat) if provided
6. Enters a REPL loop that accepts follow-up messages and slash commands (`/exit`, `/help`, `/history [n]`, `/restore <#|id>`, `/clear`)
7. Each turn preserves conversation history via the same session store
8. On restart, the session is recovered: mid-turn assistant drafts are merged into the transcript and pending `ask_*` interrupts are reported so the user knows what the agent was waiting for

**Source:** [`owly/src/shell.rs`](../owly/src/shell.rs) — interactive REPL, [`owly/src/startup.rs`](../owly/src/startup.rs) — mode resolution, [`owly/src/onboarding.rs`](../owly/src/onboarding.rs) — credential wizard, [`owly/src/session.rs`](../owly/src/session.rs) — checkpoint persistence and recovery.

**Source:** [`owly/src/commands.rs`](../owly/src/commands.rs) — ported from OpenWiki `src/commands.ts`.

### 4. Agent Layer — [`agent.rs`](../owly/src/agent.rs)

The core integration with `elph-agent` and `elph-ai`. Key functions:

- **`resolve_model_and_auth()`** — Extracted helper that resolves the model from config, obtains authentication, and returns the model handle, models arc, and stream function.
- **`create_event_subscriber()`** — Extracted factory that returns an `AgentListener` closure for streaming display. Controls spinner, text deltas, thinking deltas, and tool call logging based on `stream` and `verbose` flags.
- **`run_agent()`** — Accepts a `RunAgentOptions` struct. Sets up the agent with tools, subscribes to streaming events, sends prompts, waits for completion, saves session messages, detects docs changes, and returns a `RunAgentResult`.
- **`prepare_init_command()`** — Creates system prompt + init user prompt.
- **`prepare_update_command()`** — Creates system prompt + update user prompt (includes git summary).
- **`prepare_chat_command()`** — Creates system prompt + chat user prompt.
- **`create_checkpoint_write_subscriber()`** — Factory that returns an `AgentListener` for persisting mid-turn state. Now takes a `quiet` flag for controlling warning output. Handles events including: `TextDelta` (assistant draft), `ToolExecutionStart` (records interrupt for ask tools), `ToolExecutionUpdate` (records streaming tool partial output), and `ToolExecutionEnd` (records resume/tool result). Uses `is_ask_tool()` from session.rs to detect interactive tools.

**`RunAgentResult` struct** captures the outcome of a single agent invocation:

- `completion_message` — final message text (or empty if streamed)
- `docs_changed` — whether documentation content was modified
- `skipped` — whether the run was a no-op

**`RunAgentOptions` struct** replaces the earlier positional-parameter approach. Fields: `command`, `system_prompt`, `user_prompt`, `config`, `cwd`, `print_mode`, `stream`, `verbose`, `session`, `is_followup`, `docs_snapshot_before`.

**Tool selection:**

- Init/update mode: all tools (`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`)
- Chat mode (single-turn): read-only tools (`read`, `grep`, `find`, `ls`)
- Chat mode (interactive): read-only tools + `ask_text`, `ask_select`, `ask_confirm` (from [`ask_user.rs`](../owly/src/ask_user.rs))

The tool names are appended to the system prompt after tool selection, forming a line like `Available tools for this session: read, bash, edit, write, grep, find, ls`.

**Session integration:** When a `SessionStore` is provided, the agent restores previous messages (for follow-ups or interactive chat) before starting, and saves messages after completion.

**Streaming:** The agent subscribes to `AgentEvent` variants to display progress:

- `TextDelta` — live text output (shown with `--stream` or `--verbose`)
- `ThinkingDelta` — model reasoning (shown only with `--verbose`, in dimmed gray)
- `ToolExecutionStart` / `ToolExecutionEnd` — tool call logging (in verbose mode)
- `AgentEnd` — final stats (tool call count)

**Source:** [`owly/src/agent.rs`](../owly/src/agent.rs) — ported from OpenWiki `src/agent/index.ts`.

### 5. Prompt Generation — [`prompts.rs`](../owly/src/prompts.rs)

Contains the full system prompt that defines Owly's behavior. The prompt variants include:

- **`create_system_prompt()`** — Base prompt used across all modes.
- **`create_interactive_system_prompt()`** — Extends the base prompt with instructions for interactive chat sessions: mentions the `ask_text`/`ask_select`/`ask_confirm` tools, tells the agent not to create or update docs unless asked, and defines exit commands (`/exit`, `/quit`).
- **Init/update/chat prompts** — Mode-specific user-facing text appended to the system prompt.

The base prompt includes:

- **Role definition**: Expert technical writer, software architect, product analyst
- **Run discipline**: Filesystem tool usage rules
- **Git discipline**: How to use git evidence
- **Existing documentation discipline**: How to handle existing docs
- **Security rules**: Secret redaction requirements
- **Documentation goals**: Quality standards
- **Section quality rules**: Page structure guidelines
- **Frontmatter requirements**: YAML frontmatter format

This instruction set guides the LLM's documentation behavior.

**Source:** [`owly/src/prompts.rs`](../owly/src/prompts.rs) — ported from OpenWiki `src/agent/prompt.ts`.

### 6. Configuration — [`config.rs`](../owly/src/config.rs)

The `Config` struct holds resolved provider, model ID, and working directory. `Config::resolve()`:

1. Checks `--model` flag (supports `provider/model` format)
2. Falls back to `OWLY_PROVIDER` / `OWLY_MODEL_ID` env vars
3. Falls back to auto-detection based on available API keys
4. Validates provider exists in known provider list
5. Warns if API key is missing but doesn't fail (agent will error with a clear message)

Also supports `~/.owly/config.json` for persistent settings.

**Source:** [`owly/src/config.rs`](../owly/src/config.rs) — ported from OpenWiki `src/constants.ts` and `src/env.ts`.

### 7. Provider Registry — [`constants.rs`](../owly/src/constants.rs)

Defines all supported LLM providers with their display labels and API key environment variables. See [configuration page](configuration.md) for the full list.

**Provider auto-detection:** Checks environment variables in priority order: `OPENCODE_API_KEY` → `ANTHROPIC_API_KEY` → `OPENAI_API_KEY` → etc.

**Source:** [`owly/src/constants.rs`](../owly/src/constants.rs).

### 8. Documentation Management — [`docs.rs`](../owly/src/docs.rs)

Handles reading/writing documentation files with frontmatter, creating snapshots for change detection, and saving update metadata.

**Snapshot system:** Before an update, a hash-based snapshot is taken of all `openwiki/` files. After the run, the new snapshot is compared to detect changes.

**Source:** [`owly/src/docs.rs`](../owly/src/docs.rs) — ported from OpenWiki `src/agent/utils.ts`.

### 9. Metadata Tracking — [`metadata.rs`](../owly/src/metadata.rs)

Tracks the last successful update in `openwiki/.last-update.json`. The no-op check:

1. Loads last update metadata
2. Compares current git HEAD to last known HEAD
3. Checks `git status --short` for uncommitted changes
4. Skips update if only `openwiki/` files changed since last HEAD

**Source:** [`owly/src/metadata.rs`](../owly/src/metadata.rs).

### 10. Supporting Modules

| Module                | Responsibility                                                                                                                                                                                                                                                                                                                                                                                               | Source                                                            |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `ask_user.rs`         | Interactive tools: `ask_text`, `ask_select`, `ask_confirm` for multi-turn chat                                                                                                                                                                                                                                                                                                                               | [`owly/src/ask_user.rs`](../owly/src/ask_user.rs)                 |
| `checkpoint.rs`       | Turso-backed checkpoint persistence (`TursoCheckpointSaver`, port of langgraph-checkpoint) — supports mid-turn draft (`ASSISTANT_DRAFT`), interrupt/resume tracking (`INTERRUPT`/`RESUME`), and streaming tool partial output (`TOOL_PARTIAL`)                                                                                                                                                               | [`owly/src/checkpoint.rs`](../owly/src/checkpoint.rs)             |
| `credentials.rs`      | Loads `~/.owly/.env`, applies to process environment, secures directory permissions                                                                                                                                                                                                                                                                                                                          | [`owly/src/credentials.rs`](../owly/src/credentials.rs)           |
| `ecosystem.rs`        | Repository ecosystem hooks — syncs Owly context to `AGENTS.md` / `CLAUDE.md`                                                                                                                                                                                                                                                                                                                                 | [`owly/src/ecosystem.rs`](../owly/src/ecosystem.rs)               |
| `env.rs`              | Environment validation, base URL checks, debug logging (`OWLY_DEBUG`)                                                                                                                                                                                                                                                                                                                                        | [`owly/src/env.rs`](../owly/src/env.rs)                           |
| `frontmatter.rs`      | Parses/generates YAML frontmatter                                                                                                                                                                                                                                                                                                                                                                            | [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs)           |
| `diagnostics.rs`      | Redacts secrets from error output, detects provider 500s                                                                                                                                                                                                                                                                                                                                                     | [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs)           |
| `onboarding.rs`       | First-run credential wizard (provider selection, API key, base URL, model)                                                                                                                                                                                                                                                                                                                                   | [`owly/src/onboarding.rs`](../owly/src/onboarding.rs)             |
| `session.rs`          | Turso-backed session store with thread identity, message persistence, and crash recovery. Provides `SessionStore` (load/conversation/save/reset), `TurnWriteContext` (with `record_interrupt`/`record_resume`/`record_tool_partial` for ask-tool persistence), `LoadedConversation`/`SessionRecovery` types, and `merge_recovery_messages()` for restoring mid-turn drafts and pending interrupts on restart | [`owly/src/session.rs`](../owly/src/session.rs)                   |
| `shell.rs`            | Interactive Owly shell — credential setup, initial command, REPL loop                                                                                                                                                                                                                                                                                                                                        | [`owly/src/shell.rs`](../owly/src/shell.rs)                       |
| `startup.rs`          | Startup mode resolution (non-interactive vs. interactive), TTY validation                                                                                                                                                                                                                                                                                                                                    | [`owly/src/startup.rs`](../owly/src/startup.rs)                   |
| `ui_events.rs`        | Agent→TUI event bridge (`AgentUiEvent` enum for streaming progress)                                                                                                                                                                                                                                                                                                                                          | [`owly/src/ui_events.rs`](../owly/src/ui_events.rs)               |
| `tui/context.rs`      | Thread-safe `AppContext` for TUI and async dispatch                                                                                                                                                                                                                                                                                                                                                          | [`owly/src/tui/context.rs`](../owly/src/tui/context.rs)           |
| `tui/entries.rs`      | Typed transcript entries (`OwlyEntry`, `OwlyEntryKind`)                                                                                                                                                                                                                                                                                                                                                      | [`owly/src/tui/entries.rs`](../owly/src/tui/entries.rs)           |
| `tui/chat_stream.rs`  | Scrollable transcript with keyboard navigation and typed entry rendering                                                                                                                                                                                                                                                                                                                                     | [`owly/src/tui/chat_stream.rs`](../owly/src/tui/chat_stream.rs)   |
| `tui/transcript.rs`   | `TranscriptApplier`: maps `AgentUiEvent` → `OwlyEntry` list updates                                                                                                                                                                                                                                                                                                                                          | [`owly/src/tui/transcript.rs`](../owly/src/tui/transcript.rs)     |
| `tui/activity.rs`     | Activity bar with live tool chips                                                                                                                                                                                                                                                                                                                                                                            | [`owly/src/tui/activity.rs`](../owly/src/tui/activity.rs)         |
| `tui/chrome.rs`       | Shared visual tokens (`subtle_border` for low-contrast frames)                                                                                                                                                                                                                                                                                                                                               | [`owly/src/tui/chrome.rs`](../owly/src/tui/chrome.rs)             |
| `tui/spinner.rs`      | Animated braille loading indicator (`LoadingSpinner` component)                                                                                                                                                                                                                                                                                                                                              | [`owly/src/tui/spinner.rs`](../owly/src/tui/spinner.rs)           |
| `tui/tool_display.rs` | Shared formatting for tool execution output (`tool_output_preview`, `tool_chip_label`, `tool_transcript_header`, `tool_transcript_body`, `truncate_chars`)                                                                                                                                                                                                                                                   | [`owly/src/tui/tool_display.rs`](../owly/src/tui/tool_display.rs) |
| `utils.rs`            | HTML tag stripping utility                                                                                                                                                                                                                                                                                                                                                                                   | [`owly/src/utils.rs`](../owly/src/utils.rs)                       |

---

## Agent Execution Flow (Init/Update, Non-Interactive)

```
1. CLI parses args → Command::Init or Command::Update
2. run_command() → startup::resolve_startup_mode() → NonInteractive
3. Credentials loaded from ~/.owly/.env
4. Config resolved (provider, model, cwd)
5. Environment validated (API key check, base URL check)
6. SessionStore opened (Turso checkpoint, thread ID based on cwd hash)
7. Documentation snapshot taken (before state for change detection)
8. System prompt built from prompts.rs + mode-specific instructions
9. User prompt built with #create_runtime_note() appended:
   - Init: repository context instructions
   - Update: last update metadata + git change summary
10. Agent created with:
    - System prompt (with available tool list appended)
    - Model (resolved via elph-ai)
    - Tools (all tools for init/update)
    - Session (restored messages if any)
11. Event subscriptions attached (streaming display, controlled by `stream` and `verbose` flags)
12. User prompt sent to agent
13. Agent executes: thinks, calls tools (read files, write docs)
14. On completion: session messages saved to checkpoint
15. Docs snapshot compared to detect changes
16. If docs changed: metadata saved to .last-update.json,
    ecosystem hooks synced (AGENTS.md / CLAUDE.md)
```

---

## Change Guidance

### Adding a new provider

1. Add entry to `provider_config()` in [`constants.rs`](../owly/src/constants.rs)
2. Add to `all_providers()` list
3. Add API key env var to `MANAGED_ENV_KEYS` in [`credentials.rs`](../owly/src/credentials.rs)
4. Add to auto-detect chain in `resolve_configured_provider()` in [`constants.rs`](../owly/src/constants.rs)
5. Add to `API_KEY_ENV_VARS` in [`diagnostics.rs`](../owly/src/diagnostics.rs) for redaction

### Modifying agent behavior

- **Prompts** are in [`prompts.rs`](../owly/src/prompts.rs) — base system prompt, interactive prompt, init/update/chat templates, plus `create_runtime_note()` appended to all user prompts
- **Tool selection** by mode happens in [`agent.rs`](../owly/src/agent.rs) (`create_all_tools` vs `create_read_only_tools`); chat mode adds `ask_user` tools via `create_ask_text_tool()`, `create_ask_select_tool()`, `create_ask_confirm_tool()`; tool names are appended to the system prompt after selection
- **Streaming vs verbose**: `--stream` shows `TextDelta` only; `--verbose` shows everything including `ThinkingDelta` and tool call logs; controlled by the `stream` and `verbose` fields in `RunAgentOptions`
- **Event handling** for streaming display is in the `create_event_subscriber()` factory function, extracted from the inline closure in `run_agent()`
- **Interactive mode** is managed by [`shell.rs`](../owly/src/shell.rs) (`ShellOptions` → `run()`), which orchestrates credential wizard, session setup, initial command execution, and the REPL loop
- **Session persistence** is handled by [`session.rs`](../owly/src/session.rs) (`SessionStore`), backed by `TursoCheckpointSaver` in [`checkpoint.rs`](../owly/src/checkpoint.rs). The checkpoint subscriber in `create_checkpoint_write_subscriber()` persists mid-turn assistant drafts, streaming tool partial output (`TOOL_PARTIAL`), and interrupt/resume records for ask tools. On restart, `load_conversation()` calls `merge_recovery_messages()` to restore drafts and report pending interrupts.
- **Debug logging** can be enabled via `OWLY_DEBUG=1` — uses `env::debug_log()` which outputs `[debug]` prefixed lines to stderr

### Adding a new provider

1. Add entry to `provider_config()` in [`constants.rs`](../owly/src/constants.rs)
2. Add to `all_providers()` list
3. Add API key env var to `MANAGED_ENV_KEYS` in [`credentials.rs`](../owly/src/credentials.rs)
4. Add to auto-detect chain in `resolve_configured_provider()` in [`constants.rs`](../owly/src/constants.rs)
5. Add to `API_KEY_ENV_VARS` in [`diagnostics.rs`](../owly/src/diagnostics.rs) for redaction
6. Optionally add to `ONBOARDING_PROVIDERS` in [`constants.rs`](../owly/src/constants.rs) for the first-run wizard

### Adding a new command

1. Add variant to `Command` enum in [`commands.rs`](../owly/src/commands.rs)
2. Add handler in `run_non_interactive()` and/or `InitialRun` in [`startup.rs`](../owly/src/startup.rs)
3. Add CLI flag in [`cli.rs`](../owly/src/cli.rs)
4. Add prompt preparation function in [`agent.rs`](../owly/src/agent.rs)

### Adding a new interactive tool

1. Add a creation function in [`ask_user.rs`](../owly/src/ask_user.rs) using `simple_tool()`
2. Import and push it in the tool setup section of `run_agent()` in [`agent.rs`](../owly/src/agent.rs)

### Relevant tests

When modifying any of these areas, run the corresponding tests:

| Area                 | Test File(s)                                                                                                      |
| -------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Agent commands       | [`agent_test.rs`](../owly/tests/agent_test.rs)                                                                    |
| Session / checkpoint | [`checkpoint_test.rs`](../owly/tests/checkpoint_test.rs), [`session_test.rs` (in-source)](../owly/src/session.rs) |
| Config resolution    | [`config_test.rs`](../owly/tests/config_test.rs)                                                                  |
| Frontmatter          | [`frontmatter_ext_test.rs`](../owly/tests/frontmatter_ext_test.rs)                                                |
| Metadata/no-op       | [`metadata_ext_test.rs`](../owly/tests/metadata_ext_test.rs)                                                      |
| Prompts              | [`prompts_test.rs`](../owly/tests/prompts_test.rs)                                                                |
| Secret redaction     | [`redaction_ext_test.rs`](../owly/tests/redaction_ext_test.rs)                                                    |
| Environment          | [`env_ext_test.rs`](../owly/tests/env_ext_test.rs)                                                                |
| Documentation files  | [`docs_test.rs`](../owly/tests/docs_test.rs)                                                                      |
