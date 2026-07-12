---
title: "Architecture"
last_updated: 2026-07-30T10:00:00Z
category: architecture
tags:
    - architecture
    - design
    - modules
status: published
---

# Architecture

## Overview

Owly is a CLI agent that generates and maintains documentation in either **code** mode (repository `openwiki/`) or **personal** mode (`~/.owly/wiki/`). It follows a pipeline: **CLI → Mode → Command → Agent → LLM → Filesystem**.

```
User Input (CLI flags + trailing args)
    │
    ▼
┌──────────────────┐
│     cli.rs       │  -- init/update/chat flags, mode positional, product subcommands
│  (arg parsing)   │  -- resolves RunMode (Code | Personal)
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│   mode.rs        │  WikiContext: wiki_root, agent_cwd, session_anchor
│ (RunMode, ctx)   │  code → ./openwiki/ , personal → ~/.owly/wiki/
└────────┬─────────┘
         │
         ▼
┌──────────────────┐     ┌────────────────────┐
│ commands/mod.rs  │────▶│  commands/doc_run  │  shared init/update runs
│  (dispatch)      │     │  (code + personal) │
│                  │     └────────────────────┘
│  mode-aware      │     ┌────────────────────────┐
│  prompt prep     │────▶│  agent/mod.rs          │
└────────┬─────────┘     │  (tools, run, model)   │
         │               └───────────┬────────────┘
         │                           │
         ▼                           ▼
┌──────────────────────┐   ┌──────────────────────┐
│ credentials/env      │   │ elph-agent + elph-ai │
│ (~/.owly, OAuth)     │   │ (LLM, tools, stream) │
└──────────────────────┘   └───────────┬──────────┘
                                       │
                                       ▼
                              ┌────────────────────┐
                              │    Filesystem      │
                              │  (wiki_root/*)     │
                              └────────────────────┘
```

---

## Module Architecture

### 1. Entrypoint — [`main.rs`](../owly/src/main.rs)

Initializes `tracing` logging, parses CLI arguments via `clap`, and calls `cli.execute()`.

### 2. CLI Layer & Mode Resolution — [`cli.rs`](../owly/src/cli.rs), [`mode.rs`](../owly/src/mode.rs)

The `Cli` struct (clap derive) parses arguments. Key flags:

- `--init` / `--update` — select documentation action
- `--mode` — `code` or `personal` (or as positional arg)
- `--model` — override provider/model
- `--print` / `--stream` / `--verbose` — output control
- `--directory` — working directory
- `--dry-run` — plan only, no LLM calls
- `--credentials` — print credential diagnostics and exit
- Trailing arguments — chat message or product subcommand (`auth`, `ingest`, `cron`)

The `execute()` method first resolves the run mode:

1. Checks `--mode` flag
2. Checks positional arg (`code` or `personal`)
3. Defaults to `Personal`

Then creates a `WikiContext` from the resolved mode and `cwd`, resolves the `Command` (Init/Update/Chat), and calls [`run_command()`](../owly/src/commands/mod.rs) with the context. Product subcommands (`auth`/`ingest`/`cron`) are parsed from trailing args before command resolution — delegated to [`cli_product.rs`](../owly/src/cli_product.rs).

**Bare invocation** (`owly` with no flags/args) prints "Interactive mode not yet implemented".

**Banner output** uses ANSI color codes (cyan for logo, green for values, dimmed for labels).

**Source:** [`owly/src/cli.rs`](../owly/src/cli.rs) — ported from OpenWiki `src/cli.tsx`. [`owly/src/mode.rs`](../owly/src/mode.rs) — `RunMode` and `WikiContext` types.

### 3. Command Dispatch — [`commands/mod.rs`](../owly/src/commands/mod.rs)

`run_command()` takes a `WikiContext` and resolves the environment:

1. Calls `ctx.ensure_layout()` to verify or create wiki directories
2. Loads credentials via `credentials::load_env()`
3. Resolves `Config` from model override and repo_cwd
4. Runs `ensure_provider_setup()` for interactive credential wizard if needed
5. Validates non-interactive input via `startup::validate_non_interactive()`
6. Sets up environment via `env::setup_environment()`
7. Delegates to `non_interactive::run_non_interactive()`

The `run_non_interactive()` function dispatches to mode-specific logic:

| Command            | Behavior                                                                                                                       |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| `Init`             | Delegates to [`doc_run::run_init_agent()`](../owly/src/commands/doc_run.rs) — checks wiki root, runs init agent                |
| `Update`           | Delegates to [`doc_run::run_update_agent()`](../owly/src/commands/doc_run.rs) — checks wiki root, runs update agent            |
| `Chat { message }` | Single-turn chat using read-only tools via `agent::run_agent()`. Interactive multi-turn chat requires passing a `SessionStore` |

Each non-interactive command flows through:

1. Documentation snapshot taken (before state for change detection)
2. Mode-aware system + user prompts built by [`agent/commands.rs`](../owly/src/agent/commands.rs) — dispatches to code-mode or personal-mode prompt templates
3. Agent created and run with session and snapshot
4. Docs snapshot compared to detect changes
5. If changed: metadata saved, ecosystem hooks synced

**Source:** [`owly/src/commands/mod.rs`](../owly/src/commands/mod.rs) — ported from OpenWiki `src/commands.ts`. The [`doc_run.rs`](../owly/src/commands/doc_run.rs) sub-module holds shared init/update agent run logic. The [`non_interactive.rs`](../owly/src/commands/non_interactive.rs) sub-module handles one-shot execution.

### Auto session naming

Owly assigns a human-readable title to each chat thread, similar to the [pi-auto-session-name](https://github.com/therynamo/pi-auto-session-name) extension.

**Trigger:** After the first **chat** turn completes (`run_agent` with `command == "chat"`), when the thread has no display name and has not been auto-named before.

**Generation:** `elph_agent::generate_session_name()` collects user messages from the transcript, calls the session model via `complete_simple`, and sanitizes the result (max 60 characters, quotes stripped). Logic lives in `elph-agent` under `prompt/builtin/session_name.rs` and `prompt/session_name.rs`.

**Storage:** `TursoCheckpointSaver` persists `display_name` and `auto_named` in a `thread_metadata` table (keyed by `thread_id`). `SessionStore::display_name()`, `set_display_name()`, and `try_auto_name()` wrap read/write.

**Output:** The title is printed in the terminal output after the first turn. On launch, an existing title is loaded from the DB; if none exists, the raw thread id is used until auto-naming runs.

**Manual override:** The title is set once per thread. There is no REPL `/name` command in the current terminal-only mode (the interactive TUI shell was removed).

**Source:** [`owly/src/agent/run.rs`](../owly/src/agent/run.rs) (post-turn hook), [`owly/src/session/store.rs`](../owly/src/session/store.rs) (API), [`owly/src/checkpoint/saver/thread_meta.rs`](../owly/src/checkpoint/saver/thread_meta.rs) (persistence).

### 4. Agent Layer — [`agent/mod.rs`](../owly/src/agent/mod.rs)

The core integration with `elph-agent` and `elph-ai`. Key functions live across sub-modules:

- **`shared_models()`** — Builds a shared `elph_ai::Models` instance with a credential store for OAuth and API key providers.
- **`resolve_model_and_auth()`** — Resolves the model from Config, obtains authentication (spinner for auth resolution), and returns model handle, models arc, and stream function. Handles OAuth-only providers.
- **`create_event_subscriber()`** — Returns an `AgentListener` closure for streaming display. Uses an indicatif spinner, controls text deltas, thinking deltas, and tool call logging based on `stream` and `verbose` flags. No TUI dependency.
- **`create_checkpoint_write_subscriber()`** — Returns an `AgentListener` for persisting mid-turn state. Handles events: `TextDelta` (assistant draft), `ToolExecutionStart` (records interrupt for ask tools), `ToolExecutionUpdate` (records streaming tool partial output), and `ToolExecutionEnd` (records resume/tool result). Uses `is_ask_tool()` from session.rs.
- **`run_agent()`** — Accepts a `RunAgentOptions` struct. Sets up the agent with tools, subscribes to streaming events, sends prompts, waits for completion, saves session messages, detects docs changes, and returns `RunAgentResult`.
- **`prepare_init_command()`** — Mode-aware: dispatches to code or personal init prompts based on `WikiContext.mode`.
- **`prepare_update_command()`** — Mode-aware: dispatches to code or personal update prompts, includes git summary for code mode.
- **`prepare_chat_command()`** — Mode-aware: dispatches to code or personal chat prompts.

**`RunAgentResult` struct:**

- `completion_message` — final message text (or empty if streamed)
- `docs_changed` — whether documentation content was modified
- `skipped` — whether the run was a no-op

**`RunAgentOptions` struct** fields: `command`, `system_prompt`, `user_prompt`, `config`, `ctx` (WikiContext), `print_mode`, `stream`, `verbose`, `session`, `is_followup`, `docs_snapshot_before`.

**Tool selection:**

- Init/update mode: all tools (`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`)
- Chat mode: read-only tools (`read`, `grep`, `find`, `ls`) plus `ask_text`, `ask_select`, `ask_confirm` (dialoguer-based, from [`ask_user/mod.rs`](../owly/src/ask_user/mod.rs))

Tool names are appended to the system prompt after selection.

**Session integration:** When a `SessionStore` is provided, messages are restored before starting and saved after completion. For chat turns, `run_agent()` calls `SessionStore::try_auto_name()` when no display name exists — see [Auto session naming](#auto-session-naming).

**Streaming:** Subscribes to `AgentEvent` variants:

- `TextDelta` — live text output (shown with `--stream` or `--verbose`)
- `ThinkingDelta` — model reasoning (shown only with `--verbose`, in dimmed gray)
- `ToolExecutionStart` / `ToolExecutionEnd` — tool call logging (in verbose mode)
- `AgentEnd` — final stats

**Source:** [`owly/src/agent/mod.rs`](../owly/src/agent/mod.rs), [`run.rs`](../owly/src/agent/run.rs) (execution loop), [`listeners.rs`](../owly/src/agent/listeners.rs) (event subscriptions with indicatif spinner), [`tools.rs`](../owly/src/agent/tools.rs) (tool setup), [`commands.rs`](../owly/src/agent/commands.rs) (mode-aware prompt helpers), [`model.rs`](../owly/src/agent/model.rs) (model/auth resolution), [`shared_models.rs`](../owly/src/agent/shared_models.rs) (shared credential store) — ported from OpenWiki `src/agent/index.ts`.

#### TOON prompt encoding (optional)

After a tool finishes, `elph-agent` may rewrite large JSON tool output as [TOON](https://github.com/toon-format/toon) before the model sees it. Encoding runs **after** `after_tool_call` and **before** `ToolExecutionEnd` is emitted. Owly does not set this in code — enable with `ELPH_PROMPT_ENCODING=toon` or `auto` in the environment or `~/.owly/.env`.

| Mode   | Effect                                |
| ------ | ------------------------------------- |
| `off`  | Default — unchanged tool results      |
| `toon` | Encode eligible JSON ≥ size threshold |
| `auto` | Tabular JSON arrays only              |

Full reference: [prompt-encoding.md](prompt-encoding.md), implementation in [`crates/elph-agent/src/runtime/prompt_encoding/`](../crates/elph-agent/src/runtime/prompt_encoding/).

### 5. Prompt Generation — [`prompts.rs`](../owly/src/prompts.rs)

Contains the system and user prompts that define Owly's behavior. The prompt variants include:

- **`create_system_prompt()`** — Base prompt for code mode (repository documentation).
- **`create_personal_system_prompt()`** — Base prompt for personal mode (knowledge wiki).
- **`create_chat_prompt()`**, **`create_init_prompt()`**, **`create_update_prompt()`** — Code-mode user prompts.
- **`create_personal_chat_prompt()`**, **`create_personal_init_prompt()`**, **`create_personal_update_prompt()`** — Personal-mode user prompts.
- **`create_runtime_note()`** — Appended to all user prompts to tell the agent the wiki root and runtime conventions.

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

### 7. Provider Registry — [`constants/mod.rs`](../owly/src/constants/mod.rs)

Defines all supported LLM providers with their display labels and API key environment variables. See [configuration page](configuration.md) for the full list. Sub-modules: [`providers.rs`](../owly/src/constants/providers.rs) (provider definitions), [`resolve.rs`](../owly/src/constants/resolve.rs) (auto-detection logic).

**Provider auto-detection:** Checks environment variables in priority order: `OPENCODE_API_KEY` → `ANTHROPIC_API_KEY` → `OPENAI_API_KEY` → etc.

**Source:** [`owly/src/constants/mod.rs`](../owly/src/constants/mod.rs).

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

| Module                 | Responsibility                                                                                                                                                                                                          | Source                                                              |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `ask_user/`            | Interactive tools: `ask_text`, `ask_select`, `ask_confirm` (dialoguer-based bridge).                                                                                                                                    | [`owly/src/ask_user/mod.rs`](../owly/src/ask_user/mod.rs)           |
| `auth/`                | OAuth provider configuration and `owly auth list` / `owly auth configure`.                                                                                                                                              | [`owly/src/auth/mod.rs`](../owly/src/auth/mod.rs)                   |
| `checkpoint/`          | Turso-backed checkpoint persistence (`TursoCheckpointSaver`) — mid-turn drafts, interrupt/resume tracking, streaming tool partial output, per-thread display names.                                                     | [`owly/src/checkpoint/mod.rs`](../owly/src/checkpoint/mod.rs)       |
| `cli_product.rs`       | Product subcommand routing: `auth`, `ingest`, `cron`.                                                                                                                                                                   | [`owly/src/cli_product.rs`](../owly/src/cli_product.rs)             |
| `code_mode.rs`         | Code-mode repository setup — agent guidance snippets for `AGENTS.md`/`CLAUDE.md`, optional GitHub Actions workflow.                                                                                                     | [`owly/src/code_mode.rs`](../owly/src/code_mode.rs)                 |
| `connectors/`          | External data source connectors: `git_repo`, `hackernews`, `io`, `registry`, `web_search`, `x_source`.                                                                                                                  | [`owly/src/connectors/mod.rs`](../owly/src/connectors/mod.rs)       |
| `credentials/`         | `~/.owly/.env` loading, OAuth credential persistence (`OwlyCredentialStore`), auth context (`OwlyAuthContext`), OAuth login flow.                                                                                       | [`owly/src/credentials/mod.rs`](../owly/src/credentials/mod.rs)     |
| `ecosystem.rs`         | Thin re-export of `code_mode` module for `AGENTS.md`/`CLAUDE.md` sync.                                                                                                                                                  | [`owly/src/ecosystem.rs`](../owly/src/ecosystem.rs)                 |
| `env.rs`               | Environment validation, base URL checks, debug logging (`OWLY_DEBUG`).                                                                                                                                                  | [`owly/src/env.rs`](../owly/src/env.rs)                             |
| `frontmatter.rs`       | Parses/generates YAML frontmatter.                                                                                                                                                                                      | [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs)             |
| `diagnostics.rs`       | Secret redaction, provider error handling.                                                                                                                                                                              | [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs)             |
| `help_content.rs`      | Extended help text displayed by `--help`.                                                                                                                                                                               | [`owly/src/help_content.rs`](../owly/src/help_content.rs)           |
| `ingestion.rs`         | File ingestion pipeline for personal wiki indexing.                                                                                                                                                                     | [`owly/src/ingestion.rs`](../owly/src/ingestion.rs)                 |
| `instructions.rs`      | Wiki brief management (`openwiki/INSTRUCTIONS.md`): read, save, prompt user when missing.                                                                                                                               | [`owly/src/instructions.rs`](../owly/src/instructions.rs)           |
| `interactive.rs`       | Terminal feedback: dialoguer prompts and indicatif spinner. `ensure_provider_setup()` runs the credential wizard.                                                                                                       | [`owly/src/interactive.rs`](../owly/src/interactive.rs)             |
| `onboarding.rs`        | First-run credential wizard (provider selection, API key, OAuth, base URL, model).                                                                                                                                      | [`owly/src/onboarding.rs`](../owly/src/onboarding.rs)               |
| `onboarding_config.rs` | Personal wiki first-run flow (choose source connector, configure).                                                                                                                                                      | [`owly/src/onboarding_config.rs`](../owly/src/onboarding_config.rs) |
| `schedules.rs`         | Cron/timer management for scheduled wiki updates (`owly cron`).                                                                                                                                                         | [`owly/src/schedules.rs`](../owly/src/schedules.rs)                 |
| `session/`             | Turso-backed session store: thread identity, message persistence, crash recovery, display names. `SessionStore` (load/save/reset, `try_auto_name`), `TurnWriteContext`, `SessionRecovery`, `merge_recovery_messages()`. | [`owly/src/session/mod.rs`](../owly/src/session/mod.rs)             |
| `startup.rs`           | Non-interactive startup validation (credential checks, piped input).                                                                                                                                                    | [`owly/src/startup.rs`](../owly/src/startup.rs)                     |
| `utils.rs`             | HTML tag stripping utility.                                                                                                                                                                                             | [`owly/src/utils.rs`](../owly/src/utils.rs)                         |

---

## Agent Execution Flow (Init/Update, Non-Interactive)

```
1. CLI parses args → resolves RunMode (Code/Personal) via first positional or --mode
2. Creates WikiContext (wiki_root, agent_cwd, session_anchor)
3. run_command() called with WikiContext
4. ctx.ensure_layout() — creates wiki directories if needed
5. credentials::load_env() — loads ~/.owly/.env into process env
6. Config::resolve() — provider, model, cwd (checks --model, env vars, auto-detect)
7. interactive::ensure_provider_setup() — runs credential wizard if needed (TTY only)
8. startup::validate_non_interactive() — checks credentials exist for non-TTY runs
9. env::setup_environment() — validates API key / base URL
10. Documentation snapshot taken (before state for change detection)
11. Mode-aware system prompt built from prompts.rs (code or personal):
    - agent/commands.rs dispatches based on ctx.mode
12. User prompt built with create_runtime_note() appended:
    - Init: wiki brief prompt + instructions
    - Update: last update metadata + git change summary (code mode only)
13. Agent prepared with:
    - System prompt (with available tool list appended)
    - Model resolved via shared_models() + resolve_model_and_auth()
    - Tools (all tools for init/update, read-only for chat)
    - Optional SessionStore for persistence
14. Event subscriptions attached (indicatif spinner, controlled by stream/verbose)
15. User prompt sent to agent
16. Agent executes: thinks, calls tools (read files, write docs)
17. On completion: session messages saved (if session provided)
18. Docs snapshot compared to detect changes
19. If docs changed: metadata saved to wiki_root/.last-update.json,
    ecosystem hooks synced (code_mode::ensure_code_mode_repo_setup)
```

---

## Change Guidance

### Adding a new provider

1. Add entry to `provider_config()` in [`constants/providers.rs`](../owly/src/constants/providers.rs) (or the [`providers` map in `resolve.rs`](../owly/src/constants/resolve.rs))
2. Add to `all_providers()` list in [`constants/providers.rs`](../owly/src/constants/providers.rs)
3. Add API key env var to `MANAGED_ENV_KEYS` in [`credentials/mod.rs`](../owly/src/credentials/mod.rs)
4. Add to auto-detect chain in `resolve_configured_provider()` in [`constants/resolve.rs`](../owly/src/constants/resolve.rs)
5. Add to `API_KEY_ENV_VARS` in [`diagnostics.rs`](../owly/src/diagnostics.rs) for redaction
6. If OAuth-capable, add to OAuth provider list for `owly auth configure`

### Modifying agent behavior

- **Prompts** are in [`prompts.rs`](../owly/src/prompts.rs) — code and personal system prompts, init/update/chat templates, plus `create_runtime_note()` appended to all user prompts
- **Mode-aware prompt dispatch** happens in [`agent/commands.rs`](../owly/src/agent/commands.rs) — selects code or personal prompts based on `WikiContext.mode`
- **Tool selection** by mode happens in [`agent/tools.rs`](../owly/src/agent/tools.rs); chat mode adds `ask_user` tools; tool names are appended to the system prompt after selection
- **Streaming vs verbose**: `--stream` shows `TextDelta` only; `--verbose` shows everything including `ThinkingDelta` and tool call logs; controlled by the `stream` and `verbose` fields in `RunAgentOptions`
- **Event handling** for streaming display is in `create_event_subscriber()` in [`agent/listeners.rs`](../owly/src/agent/listeners.rs) — uses indicatif spinner, no TUI dependency
- **Session persistence** is handled by [`session/mod.rs`](../owly/src/session/mod.rs) (`SessionStore`), backed by `TursoCheckpointSaver` in [`checkpoint/mod.rs`](../owly/src/checkpoint/mod.rs). The checkpoint subscriber persists mid-turn drafts, tool partial output, and interrupt/resume records. On restart, `load_conversation()` calls `merge_recovery_messages()` to restore drafts.
- **Auto session naming** runs in [`agent/run.rs`](../owly/src/agent/run.rs) after chat turns; reuse or extend `elph_agent::generate_session_name`.
- **Run modes** are defined in [`mode.rs`](../owly/src/mode.rs) — when adding mode-specific behavior, check the `RunMode` enum and dispatch in `WikiContext` methods or `agent/commands.rs`.
- **Debug logging** can be enabled via `OWLY_DEBUG=1` — uses `env::debug_log()` which outputs `[debug]` prefixed lines to stderr

### Adding a new provider

1. Add entry to `provider_config()` in [`constants/providers.rs`](../owly/src/constants/providers.rs)
2. Add to `all_providers()` list in [`constants/providers.rs`](../owly/src/constants/providers.rs)
3. Add API key env var to `MANAGED_ENV_KEYS` in [`credentials/mod.rs`](../owly/src/credentials/mod.rs)
4. Add to auto-detect chain in `resolve_configured_provider()` in [`constants/resolve.rs`](../owly/src/constants/resolve.rs)
5. Add to `API_KEY_ENV_VARS` in [`diagnostics.rs`](../owly/src/diagnostics.rs) for redaction
6. Optionally add to `ONBOARDING_PROVIDERS` in [`constants/providers.rs`](../owly/src/constants/providers.rs) for the first-run wizard
7. If the provider uses OAuth, add it to the auth module in [`auth/`](../owly/src/auth/)

### Adding a new command

1. Add variant to `Command` enum in [`commands/mod.rs`](../owly/src/commands/mod.rs)
2. Add handler in `run_non_interactive()` in [`commands/non_interactive.rs`](../owly/src/commands/non_interactive.rs)
3. For init/update actions, add logic in [`commands/doc_run.rs`](../owly/src/commands/doc_run.rs)
4. Add CLI flag in [`cli.rs`](../owly/src/cli.rs)
5. Add mode-aware prompt preparation function in [`agent/commands.rs`](../owly/src/agent/commands.rs)
6. For product subcommands (auth/ingest/cron), route through [`cli_product.rs`](../owly/src/cli_product.rs) instead

### Adding a new interactive tool

1. Add a creation function in [`ask_user/mod.rs`](../owly/src/ask_user/mod.rs) using `simple_tool()`
2. Add to `ASK_TOOL_NAMES` constant in the same module
3. Import and push it in `create_ask_tools()` function
4. Wire into tool setup in `run_agent()` in [`agent/run.rs`](../owly/src/agent/run.rs)

### Relevant tests

When modifying any of these areas, run the corresponding tests:

| Area                 | Test File(s)                                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Agent commands       | [`agent_test.rs`](../owly/tests/agent_test.rs)                                                                         |
| Session / checkpoint | [`checkpoint_test.rs`](../owly/tests/checkpoint_test.rs), [`session_name_test.rs`](../owly/tests/session_name_test.rs) |
| Config resolution    | [`config_test.rs`](../owly/tests/config_test.rs)                                                                       |
| Frontmatter          | [`frontmatter_ext_test.rs`](../owly/tests/frontmatter_ext_test.rs)                                                     |
| Metadata/no-op       | [`metadata_ext_test.rs`](../owly/tests/metadata_ext_test.rs)                                                           |
| Prompts              | [`prompts_test.rs`](../owly/tests/prompts_test.rs)                                                                     |
| Secret redaction     | [`redaction_ext_test.rs`](../owly/tests/redaction_ext_test.rs)                                                         |
| Environment          | [`env_ext_test.rs`](../owly/tests/env_ext_test.rs)                                                                     |
| Documentation files  | [`docs_test.rs`](../owly/tests/docs_test.rs)                                                                           |
| Personal mode        | [`personal_mode_test.rs`](../owly/tests/personal_mode_test.rs)                                                         |
