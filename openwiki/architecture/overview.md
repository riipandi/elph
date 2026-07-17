---
type: Reference
title: Architecture Overview
description: Layered crate architecture for the Elph AI agent workspace — module map, design principles, and key architectural decisions.
tags: [architecture, elph, crate-layout, design]
---

# Architecture Overview

Elph is a Rust workspace of layered crates designed for building AI agent applications. The primary product is a coding agent CLI + TUI, but the runtime libraries are app-agnostic.

## Layer diagram

```
┌─────────────────────────────────────────────────────┐
│                   elph (binary)                      │
│  CLI · Shell (TUI) · Agent orchestration ·          │
│  Platform (paths, settings, databases, extensions)   │
├─────────────────────────────────────────────────────┤
│                  elph-tui (crate)                    │
│  iocraft component library · 13+ examples, tests    │
│  (primary TUI in elph binary)                       │
├─────────────────────────────────────────────────────┤
│                 elph-agent (crate)                   │
│  AgentHarness · Session · Compaction · Goals ·      │
│  Subagents · Skills · MCP Client · WASM Plugins     │
├─────────────────────────────────────────────────────┤
│                   elph-ai (crate)                    │
│  Model catalog · Provider APIs · Auth/OAuth ·       │
│  Image generation · Web tools · Faux provider       │
├─────────────────────────────────────────────────────┤
│                 elph-core (crate)                    │
│  Floppy (Turso vector store) · Logger · Scaffold    │
│  Path resolution · Filesystem utilities             │
├─────────────────────────────────────────────────────┤
│                 elph-exec (crate)                    │
│  PTY shell execution · Configurable timeouts ·      │
│  Abort tokens · Streaming output · Sanitization     │
└─────────────────────────────────────────────────────┘
```

## Design principles

1. **Minimal agent CLI** — one interactive binary (`elph`), non-interactive `run`, and admin subcommands.
2. **Native tool calling** — models invoke tools via provider APIs; text markup is fallback only.
3. **Thin binary** — `main.rs` only parses CLI and exits; the library crate (`elph/src/lib.rs`) holds modules for testability.
4. **Platform vs product** — `platform/` owns paths, settings, bootstrap, and datastore; no agent logic.
5. **Shell vs agent** — `tui/` is the interactive TUI app; `agent/` is the coding session runtime.
6. **Pi-compatible** — architectural concepts ported from [pi](https://pi.dev) TypeScript packages.

## Crate details

### `elph` (binary crate)

Path: `/elph/`

The product shell. Wires together the agent runtime, TUI, CLI, and platform concerns.

Key modules:

- `src/cli/` — Subcommands: `run`, `acp`, `codegraph`, `completions`, `doctor`, `export`, `import`, `mcp`, `memory`, `models`, `provider`, `server`, `session`, `stats`, `update`, `worktree` (`/elph/src/cli/mod.rs`)
- `src/tui/` — Modular iocraft-based interactive shell: `shell.rs`, `focus.rs`, `tool_approval.rs`, `user_question.rs`, `activity.rs`, `agent_bridge.rs`, `chrome/`, `prompt/`, `transcript/`, `slash_palette/`, `theme.rs`, and more (`/elph/src/tui/mod.rs`)
- `src/agent/` — Pi coding-agent equivalent: session orchestration, runtime wiring, diagnostics tool, ask_user tool, slash commands, tool policy, run mode, skills loading (`skills_load.rs`), tools catalog reconciliation (`tools_catalog.rs`), system prompt rendering (`system_prompt_slash.rs`), and the `prompt/` subdirectory (`agents_md.rs`, `builder.rs`, `modes.rs`) (`/elph/src/agent/mod.rs`)
- `src/platform/` — Host environment: paths, settings, bootstrap, datastore, MCP config, migrations, hooks, interrupt handling (`/elph/src/platform/mod.rs`)
- `src/extensions/` — WASM extension host (`/elph/src/extensions/mod.rs`)

Source reference: `/elph/src/lib.rs`, `/elph/src/main.rs`

### `elph-agent` (library crate)

Path: `/crates/elph-agent/`

App-agnostic agent runtime. Ported from `@earendil-works/pi-agent`.

Key modules:

- `agent/` — Stateful `Agent` wrapper with event subscription, queue management, harness (`agent/harness/`), and subagents (`agent/subagent/`) (`/crates/elph-agent/src/agent/mod.rs`)
- `runtime/` — Low-level turn runner (agent loop), event stream, execution env, loop config, proxy (`/crates/elph-agent/src/runtime/`)
- `collaboration/` — Collaboration modes (Plan / Default), planning, tool filtering (`/crates/elph-agent/src/collaboration/`)
- `session/` — Tree-structured session persistence with pluggable backends (filesystem, Turso, in-memory) (`/crates/elph-agent/src/session/mod.rs`)
- `compaction/` — Context window management via summarization, branch clipping, token estimation (`/crates/elph-agent/src/compaction/mod.rs`)
- `goals/` — Session goal persistence, auto-steering, accounting (`/crates/elph-agent/src/goals/mod.rs`)
- `skills/` — Skill discovery from `SKILL.md` files, argument hint parsing and validation (`args.rs`) (`/crates/elph-agent/src/skills/mod.rs`)
- `tools/` — Built-in tools: `read_file`, `bash`, `edit_file`, `write_file`, `grep`, `find_path`, `list_dir`, `create_dir`, `copy_path`, `delete_path`, `move_path`, `web_search`, `web_fetch`, collaboration tools; MCP client lives under `tools/mcp/` (`/crates/elph-agent/src/tools/mod.rs`)
- `prompt/` — MiniJinja-based template engine (`template.rs`, `system_builder.rs`, `context.rs`), builtin prompts, external prompt templates, session naming, defaults, and TOON encoding (`/crates/elph-agent/src/prompt/`)
- `plugins/` — WASM extension host (optional, feature `extensions`) (`/crates/elph-agent/src/plugins/mod.rs`)
- `messages/` — Message conversion helpers (`/crates/elph-agent/src/messages/mod.rs`)
- `types/` — Core agent types: loop config, messages, tools, enums (`/crates/elph-agent/src/types/mod.rs`)

Features: `mcp` (default), `extensions` (default), `prompt-templates` (default via `full`), `obscura` (optional), `tracing` (optional — fastrace spans), plus individual tool feature flags (`tools-read-file`, `tools-bash`, `tools-edit-file`, `tools-write-file`, `tools-grep`, `tools-find-path`, `tools-list-dir`, `tools-create-dir`, `tools-copy-path`, `tools-delete-path`, `tools-move-path`, `tools-web`, `tools-collaboration`) and convenience groups (`tools-search`, `tools-edit-tools`, `builtin-tools`).

### `elph-ai` (library crate)

Path: `/crates/elph-ai/`

Unified LLM API layer. Ported from `@earendil-works/pi-ai`.

Key modules:

- `api/` — Provider-specific API implementations: OpenAI, Azure OpenAI, Bedrock, Google, HTTP proxy (`/crates/elph-ai/src/api/`)
- `auth/` — API key and OAuth credential management (`/crates/elph-ai/src/auth/`)
- `models/` — Model catalog, provider registry, cost calculation (`/crates/elph-ai/src/models/`)
- `providers/` — Built-in provider definitions and faux provider for testing (`/crates/elph-ai/src/providers/`)
- `images/` — Image generation models (`/crates/elph-ai/src/images/`)
- `types/` — Core AI types: messages, tools, events (`/crates/elph-ai/src/types/`)
- `utils/` — Deferred tools, diagnostics, event streams, overflow handling, retry, validation (`/crates/elph-ai/src/utils/`)

### `elph-core` (library crate)

Path: `/crates/elph-core/`

Shared primitives.

Key modules:

- `floppy/` — Agent memory store: Turso-backed vector search with Welford baseline scoring and EMA weight updates. Ported from [memelord](https://github.com/glommer/memelord). (`/crates/elph-core/src/floppy/`)
- `logger/` — Structured logging via `logforth` with rotating file output and optional fastrace span attachment (`/crates/elph-core/src/logger/`)
- `trace/` — Optional fastrace distributed tracing: `JsonlReporter`, `root_span`, W3C `traceparent` propagation (`/crates/elph-core/src/trace/`)
- `scaffold/` — Bundled manifests, trust stores, version files (`/crates/elph-core/src/scaffold/`)
- `utils/` — Path resolution (`AppPaths`), project key, filesystem utilities (`/crates/elph-core/src/utils/`)
- `fs.rs` — `ensure_dirs`, `write_file_if_missing`, `write_json_file`, `write_private_file` (`/crates/elph-core/src/fs.rs`)

### `elph-tui` (library crate — iocraft component library)

Path: `/crates/elph-tui/`

> **Note**: The elph-tui crate provides iocraft-based component modules (17+ modules) and 13+ examples with integration tests. The primary TUI implementation lives in the `elph` binary crate (`elph/src/tui/`). Once the widget library stabilises, reusable widgets will be extracted back into this crate and published to crates.io.

Key modules (current — `/crates/elph-tui/src/`): `lib.rs` with 20+ component modules under `components/` (`ascii_font`, `card`, `code`, `dialog_shell/`, `diff`, `frame_buffer`, `input`, `line_numbers`, `markdown/`, `progress_indicator`, `qr_code`, `scroll_bar`, `scroll_box`, `select`, `slider`, `status_indicator`, `tab_select`, `text`, `textarea/`, `theme`). `textarea/` has `component.rs`, `input/` (paste, submit, wire_edit), `layout.rs`, `state.rs`. `markdown/` has 11+ sub-modules including `blocks`, `colors`, `highlight`, `layout`, `linkify`, `model`, `parse`, `parser_config`, `render`, `syntax`, `table`, `theme`. `dialog_shell/` provides modal-like dialog panels (confirm, multi_choice, user_input, todo_list, etc.). Additional crate-level modules: `input_prefix`, `slash_palette/` (fuzzy, keyboard, layout, model, state), `text_editing/` (actions, input, line, submit, wire), `transcript_layout.rs`, `text_input_layout.rs`, `cli_progress.rs`, `loader.rs`, `paste.rs`, `utils.rs`. Examples in `examples/` (26+ examples: weather, calculator, chat_layout, coding_agent app, demo_dialog_shell, demo_theme, etc.). Tests in `tests/` (14 files).

### `elph-exec` (library crate)

Path: `/crates/elph-exec/`

PTY-based shell execution extracted from `elph-agent`. Provides a configurable shell runner with automatic PTY-on-Unix-then-piped-fallback strategy (`prefer_pty`), configurable timeouts, abort tokens via `tokio::select!`, streaming `on_stdout`/`on_stderr` callbacks, and output sanitization (strips control chars and Unicode annotations).

Key modules:

- `shell.rs` — `resolve_shell()` (finds `/bin/bash` → `bash` on `$PATH` → `/bin/sh`), `exec_shell_command()` (PTY path with `AsyncFd` + tokio fallback)
- `pty/` — Unix raw PTY allocation via `rustix::pty` (`PtyMaster`, `Pts` with session leader pre-exec)
- `types.rs` — `ShellConfig`, `ShellExecOptions`, `ShellExecResult`
- `output.rs` — `sanitize_binary_output()` (strips ≤0x1f except tab/newline/cr, strips U+FFF9–FFFB)
- `error.rs` — `ExecError` enum: `Aborted`, `Timeout`, `ShellUnavailable`, `SpawnError`, `CallbackError`, `Unknown`

### `elph-swarm` (library crate)

Path: `/crates/elph-swarm/`

Multi-agent coordination. Early stage — minimal public API.

## Key architectural decisions (from git history)

| Decision                  | Commit              | Rationale                                                                       |
| ------------------------- | ------------------- | ------------------------------------------------------------------------------- |
| Layered crate layout      | `95ff396`           | Restructure from monolithic to `elph-agent`, `elph-ai`, `elph-core`, `elph-tui` |
| Migrate TUI to `iocraft`  | `b06c134`           | Replace `superlighttui` with richer widget framework                            |
| MCP client integration    | `810f72a`–`c15ac90` | Add streamable HTTP, session pool, OAuth, encrypted creds, validation           |
| TOON prompt encoding      | `0a0753c`           | Optional structured-data encoding for tool results to reduce tokens             |
| Auto session naming       | `2e0297f`           | Model-generated thread titles for session resumption UX                         |
| Goal system               | `db12bfb`           | Persisted session objectives with auto-steering                                 |
| Prompt module restructure | `97158ee`           | Split `prompt_templates/` into `prompt/{builtin,external,invoke}`               |
| STRICT SQLite tables      | `cc72e6b`           | Correct column types for Turso/SQLite compatibility                             |
| Subagent orchestration    | `1384531`           | Rename goal tools to snake_case, refactor ask_user                              |
| Prompt template engine       | `fdbede4`           | Replace ad-hoc string formatting with MiniJinja-based layered templates (`base.md`, `coding_base.md`, mode-specific appendixes) |
| Session tree persistence  | `95ff396`           | Tree-structured sessions with fork/branch/resume                                |

## Path resolution

Elph uses a `PathResolver` pattern (`/crates/elph-core/src/utils/path/`) with env var overrides:

| Env var            | Purpose                                        |
| ------------------ | ---------------------------------------------- |
| `ELPH_HOME`        | Config directory (default `~/.elph`)           |
| `ELPH_DATA_DIR`    | Data directory (default `~/.local/share/elph`) |
| `ELPH_PROJECT_DIR` | Project directory (default `pwd`)              |

Paths struct: `/elph/src/platform/paths.rs`

## Change guidance

When modifying any major area:

- **Agent runtime**: Tests in `/crates/elph-agent/tests/{agent_loop, harness, e2e, session, goals, subagent}.rs`
- **AI providers**: Tests in `/crates/elph-ai/tests/` — check provider-specific payloads
- **MCP**: Tests in `/crates/elph-agent/tests/{mcp_deepwiki, encrypt_string}.rs`
- **TUI**: iocraft rendering in `elph/src/tui/` (shell, focus, tool_approval, slash_palette, chrome, prompt, transcript); elph-tui tests in `crates/elph-tui/tests/`
- **Skills**: Tests in `/crates/elph-agent/tests/skills.rs`
- **Prompt encoding**: Tests in `/crates/elph-agent/tests/prompt_encoding.rs`
- **CLI**: Tests in `/elph/tests/{cli, bootstrap}.rs`
- See [testing.md](testing.md) for detailed test patterns.
