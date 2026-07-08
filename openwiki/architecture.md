---
title: "Architecture"
last_updated: 2026-07-08T10:57:29Z
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
    ▼
┌──────────┐    ┌───────────┐    ┌────────────────┐    ┌──────────┐
│  cli.rs  │───▶│commands.rs│───▶│   agent.rs     │───▶│ elph-ai  │
│ (parsing)│    │(dispatch) │    │ (prompt + run) │    │ (LLM)    │
└──────────┘    └───────────┘    └────────────────┘    └──────────┘
                                         │
                                         ▼
                                  ┌───────────────┐
                                  │ elph-agent    │
                                  │ (tools + run) │
                                  └───────────────┘
                                         │
                                         ▼
                                  ┌──────────────┐
                                  │  Filesystem  │
                                  │ (openwiki/)  │
                                  └──────────────┘
```

---

## Module Architecture

### 1. Entrypoint — [`main.rs`](../owly/src/main.rs)

Initializes `tracing` logging, parses CLI arguments via `clap`, and calls `cli.execute()`.

### 2. CLI Layer — [`cli.rs`](../owly/src/cli.rs)

Defines the `Cli` struct with clap derive macros. Supported flags:

- `--init` / `--update` — select mode
- `--model` — override provider/model
- `--print` / `--verbose` — output control
- `--directory` — working directory
- Trailing argument — chat message

The `execute()` method resolves the command enum and calls `run_command()`.

**Source:** [`owly/src/cli.rs`](../owly/src/cli.rs) — ported from OpenWiki `src/cli.tsx`.

### 3. Command Dispatch — [`commands.rs`](../owly/src/commands.rs)

Three command variants:

| Command            | Behavior                                                                                                         |
| ------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `Init`             | Checks if `openwiki/` exists. If yes, delegates to update path. If no, runs agent with init prompt.              |
| `Update`           | Checks if `openwiki/` exists. If no, delegates to init path. Checks no-op status. Runs agent with update prompt. |
| `Chat { message }` | Runs agent with chat prompt (read-only tools).                                                                   |

Each command:

1. Loads credentials from `~/.owly/.env`
2. Resolves configuration (provider, model)
3. Sets up environment
4. Prepares system + user prompts
5. Runs the agent
6. Saves update metadata on success (init/update only)

**Source:** [`owly/src/commands.rs`](../owly/src/commands.rs) — ported from OpenWiki `src/commands.ts`.

### 4. Agent Layer — [`agent.rs`](../owly/src/agent.rs)

The core integration with `elph-agent` and `elph-ai`. Key functions:

- **`run_agent()`** — Sets up the agent with tools, subscribes to streaming events, sends prompts, waits for completion.
- **`prepare_init_command()`** — Creates system prompt + init user prompt.
- **`prepare_update_command()`** — Creates system prompt + update user prompt (includes git summary).
- **`prepare_chat_command()`** — Creates system prompt + chat user prompt.

**Tool selection:**

- Init/update mode: all tools (`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`)
- Chat mode: read-only tools (`read`, `grep`, `find`, `ls`)

**Streaming:** The agent subscribes to `AgentEvent` variants to display progress:

- `TextDelta` / `ThinkingDelta` — live text in verbose mode
- `ToolExecutionStart` / `ToolExecutionEnd` — tool call logging
- `AgentEnd` — final stats

**Source:** [`owly/src/agent.rs`](../owly/src/agent.rs) — ported from OpenWiki `src/agent/index.ts`.

### 5. Prompt Generation — [`prompts.rs`](../owly/src/prompts.rs)

Contains the full system prompt that defines Owly's behavior. The prompt includes:

- **Role definition**: Expert technical writer, software architect, product analyst
- **Run discipline**: Filesystem tool usage rules
- **Git discipline**: How to use git evidence
- **Existing documentation discipline**: How to handle existing docs
- **Security rules**: Secret redaction requirements
- **Documentation goals**: Quality standards
- **Section quality rules**: Page structure guidelines
- **Frontmatter requirements**: YAML frontmatter format

This is the instruction set that guides the LLM's documentation behavior.

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

| Module           | Responsibility                                           | Source                                                  |
| ---------------- | -------------------------------------------------------- | ------------------------------------------------------- |
| `credentials.rs` | Loads `~/.owly/.env`, applies to process environment     | [`owly/src/credentials.rs`](../owly/src/credentials.rs) |
| `env.rs`         | Validates environment, provides debug info               | [`owly/src/env.rs`](../owly/src/env.rs)                 |
| `frontmatter.rs` | Parses/generates YAML frontmatter                        | [`owly/src/frontmatter.rs`](../owly/src/frontmatter.rs) |
| `diagnostics.rs` | Redacts secrets from error output, detects provider 500s | [`owly/src/diagnostics.rs`](../owly/src/diagnostics.rs) |
| `utils.rs`       | HTML tag stripping utility                               | [`owly/src/utils.rs`](../owly/src/utils.rs)             |

---

## Agent Execution Flow (Init/Update)

```
1. CLI parses args → Command::Init or Command::Update
2. Credentials loaded from ~/.owly/.env
3. Config resolved (provider, model, cwd)
4. Environment validated (API key check)
5. System prompt built from prompts.rs + mode-specific instructions
6. User prompt built:
   - Init: repository context instructions
   - Update: last update metadata + git change summary
7. Agent created with:
   - System prompt
   - Model (resolved via elph-ai)
   - Tools (all tools for init/update)
8. Event subscriptions attached (streaming display)
9. User prompt sent to agent
10. Agent executes: thinks, calls tools (read files, write docs)
11. On completion: update metadata saved to .last-update.json
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

- **Prompts** are in [`prompts.rs`](../owly/src/prompts.rs) — system prompt, init/update/chat templates
- **Tool selection** by mode happens in [`agent.rs`](../owly/src/agent.rs) (`create_all_tools` vs `create_read_only_tools`)
- **Event handling** for streaming display is in the `subscribe` closure in `run_agent()`

### Adding a new command

1. Add variant to `Command` enum in [`commands.rs`](../owly/src/commands.rs)
2. Add match arm in `run_command()`
3. Add CLI flag in [`cli.rs`](../owly/src/cli.rs)
4. Add prompt preparation function in [`agent.rs`](../owly/src/agent.rs)

### Relevant tests

When modifying any of these areas, run the corresponding tests:

| Area                | Test File(s)                                                       |
| ------------------- | ------------------------------------------------------------------ |
| Agent commands      | [`agent_test.rs`](../owly/tests/agent_test.rs)                     |
| Config resolution   | [`config_test.rs`](../owly/tests/config_test.rs)                   |
| Frontmatter         | [`frontmatter_ext_test.rs`](../owly/tests/frontmatter_ext_test.rs) |
| Metadata/no-op      | [`metadata_ext_test.rs`](../owly/tests/metadata_ext_test.rs)       |
| Prompts             | [`prompts_test.rs`](../owly/tests/prompts_test.rs)                 |
| Secret redaction    | [`redaction_ext_test.rs`](../owly/tests/redaction_ext_test.rs)     |
| Environment         | [`env_ext_test.rs`](../owly/tests/env_ext_test.rs)                 |
| Documentation files | [`docs_test.rs`](../owly/tests/docs_test.rs)                       |
