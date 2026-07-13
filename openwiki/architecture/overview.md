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
│  Warmer · Widgets · Diff engine · Keymap · Theme     │
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
└─────────────────────────────────────────────────────┘
```

## Design principles

1. **Minimal agent CLI** — one interactive binary (`elph`), non-interactive `run`, and admin subcommands.
2. **Native tool calling** — models invoke tools via provider APIs; text markup is fallback only.
3. **Thin binary** — `main.rs` only parses CLI and exits; the library crate (`elph/src/lib.rs`) holds modules for testability.
4. **Platform vs product** — `platform/` owns paths, settings, bootstrap, and datastore; no agent logic.
5. **Shell vs agent** — `shell/` is the interactive TUI app; `agent/` is the coding session runtime.
6. **Pi-compatible** — architectural concepts ported from [pi](https://pi.dev) TypeScript packages.

## Crate details

### `elph` (binary crate)

Path: `/elph/`

The product shell. Wires together the agent runtime, TUI, CLI, and platform concerns.

Key modules:

- `src/cli/` — Subcommands: `run`, `acp`, `codegraph`, `completions`, `doctor`, `export`, `import`, `mcp`, `memory`, `models`, `provider`, `server`, `session`, `stats`, `update`, `worktree` (`/elph/src/cli/mod.rs`)
- `src/shell/` — Interactive TUI application: `ElphApp` state, event loop, overlays, render, slash dispatch, transcript render (`/elph/src/shell/mod.rs`)
- `src/agent/` — Pi coding-agent equivalent: session orchestration, runtime wiring, slash commands, tool policy, run mode (`/elph/src/agent/mod.rs`)
- `src/platform/` — Host environment: paths, settings, bootstrap, datastore, MCP config, migrations, hooks, interrupt handling (`/elph/src/platform/mod.rs`)
- `src/extensions/` — WASM extension host (`/elph/src/extensions/mod.rs`)

Source reference: `/elph/src/lib.rs`, `/elph/src/main.rs`

### `elph-agent` (library crate)

Path: `/crates/elph-agent/`

App-agnostic agent runtime. Ported from `@earendil-works/pi-agent`.

Key modules:

- `agent/` — Stateful `Agent` wrapper with event subscription and queue management (`/crates/elph-agent/src/agent/mod.rs`)
- `agent_loop/` — Low-level turn runner: stream → tool call → result → repeat (`/crates/elph-agent/src/agent_loop/mod.rs`)
- `harness/` — `AgentHarness`: session-backed stateful runner with hooks, compaction, branching (`/crates/elph-agent/src/harness/mod.rs`)
- `session/` — Tree-structured session persistence with pluggable backends (filesystem, Turso, in-memory) (`/crates/elph-agent/src/session/mod.rs`)
- `compaction/` — Context window management via summarization, branch clipping, token estimation (`/crates/elph-agent/src/compaction/mod.rs`)
- `goals/` — Session goal persistence, auto-steering, accounting (`/crates/elph-agent/src/goals/mod.rs`)
- `subagent/` — Multi-agent orchestration: spawn, control, event forwarding (`/crates/elph-agent/src/subagent/mod.rs`)
- `skills/` — Skill discovery from `SKILL.md` files (`/crates/elph-agent/src/skills/mod.rs`)
- `tools/` — Built-in tools: `read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`, `websearch`, `webfetch`, multi-agent tools (`/crates/elph-agent/src/tools/mod.rs`)
- `mcp/` — MCP client with stdio, HTTP, SSE, OAuth, encryption, validation, policy, sessions (`/crates/elph-agent/src/mcp/mod.rs`)
- `mode/` — Collaboration modes (Plan / Default), tool filtering (`/crates/elph-agent/src/mode/mod.rs`)
- `prompt/` — Builtin prompts, external prompt templates, session naming (`/crates/elph-agent/src/prompt/mod.rs`)
- `runtime/` — Prompt encoding (TOON), `block_on` helpers (`/crates/elph-agent/src/runtime/mod.rs`)
- `plugins/` — WASM extension host (optional, feature `extensions`) (`/crates/elph-agent/src/plugins/mod.rs`)
- `env/` — `LocalExecutionEnv` for filesystem sandboxing (`/crates/elph-agent/src/env/mod.rs`)
- `sandbox/` — Zerobox-powered sandboxed execution policies (`/crates/elph-agent/src/sandbox/mod.rs`)
- `messages/` — Message conversion helpers (`/crates/elph-agent/src/messages/mod.rs`)
- `types/` — Core agent types: loop config, messages, tools, enums (`/crates/elph-agent/src/types/mod.rs`)

Features: `mcp` (default), `extensions` (default), `obscura` (optional).

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
- `logger/` — Log rotation and formatting (`/crates/elph-core/src/logger/`)
- `scaffold/` — Bundled manifests, trust stores, version files (`/crates/elph-core/src/scaffold/`)
- `utils/` — Path resolution (`AppPaths`), project key, filesystem utilities (`/crates/elph-core/src/utils/`)
- `fs.rs` — `ensure_dirs`, `write_file_if_missing`, `write_json_file`, `write_private_file` (`/crates/elph-core/src/fs.rs`)

### `elph-tui` (library crate)

Path: `/crates/elph-tui/`

Reusable terminal UI components built on [tuie](https://crates.io/crates/tuie). Migrated from `superlighttui` in commit `b06c134`.

Key modules:

- `agent/` — Agent-facing state types: collapse, model selector, OAuth, plan confirmation, session selector, tool approval, tree navigator (`/crates/elph-tui/src/agent/`)
- `chrome/` — Activity state, banner, tips (`/crates/elph-tui/src/chrome/`)
- `diff/` — Differential rendering engine: edit/diff views, markdown rendering, autocomplete, overlays, scrollback (`/crates/elph-tui/src/diff/`)
- `keymap/` — Global chord handler, shell action dispatch (`/crates/elph-tui/src/keymap/`)
- `prompt/` — Prompt state, chat stream, slash commands, thinking level, submit modes (`/crates/elph-tui/src/prompt/`)
- `runtime/` — Runtime configuration, shell startup (`/crates/elph-tui/src/runtime/`)
- `shell/` — `AgentShell`, `ShellHost` trait, `ShellChromeData` (`/crates/elph-tui/src/shell/`)
- `terminal/` — Keyboard enhancement, SIGINT handling (`/crates/elph-tui/src/terminal/`)
- `theme/` — Theme and palette management (`/crates/elph-tui/src/theme/`)
- `transcript/` — Streaming buffer, transcript entries, tool execution states (`/crates/elph-tui/src/transcript/`)
- `widgets/` — `PromptPane`, `TranscriptPane`, `SidebarPlaceholder`, `StreamingText`, `CommandPaletteState`, chrome/footer builders (`/crates/elph-tui/src/widgets/`)
- `utils/` — Display width utilities, path helpers, ANSI stripping (`/crates/elph-tui/src/utils/`)

### `elph-swarm` (library crate)

Path: `/crates/elph-swarm/`

Multi-agent coordination. Early stage — minimal public API.

## Key architectural decisions (from git history)

| Decision                  | Commit              | Rationale                                                                       |
| ------------------------- | ------------------- | ------------------------------------------------------------------------------- |
| Layered crate layout      | `95ff396`           | Restructure from monolithic to `elph-agent`, `elph-ai`, `elph-core`, `elph-tui` |
| Migrate TUI to `tuie`     | `b06c134`           | Replace `superlighttui` with richer widget framework                            |
| MCP client integration    | `810f72a`–`c15ac90` | Add streamable HTTP, session pool, OAuth, encrypted creds, validation           |
| TOON prompt encoding      | `0a0753c`           | Optional structured-data encoding for tool results to reduce tokens             |
| Auto session naming       | `2e0297f`           | Model-generated thread titles for session resumption UX                         |
| Goal system               | `db12bfb`           | Persisted session objectives with auto-steering                                 |
| Prompt module restructure | `97158ee`           | Split `prompt_templates/` into `prompt/{builtin,external,invoke}`               |
| STRICT SQLite tables      | `cc72e6b`           | Correct column types for Turso/SQLite compatibility                             |
| Subagent orchestration    | `1384531`           | Rename goal tools to snake_case, refactor ask_user                              |
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
- **TUI**: Tests in `/crates/elph-tui/tests/{tuie_shell, agent_demo}.rs`
- **Skills**: Tests in `/crates/elph-agent/tests/skills.rs`
- **Prompt encoding**: Tests in `/crates/elph-agent/tests/prompt_encoding.rs`
- **CLI**: Tests in `/elph/tests/{cli, bootstrap, sigint, shell_host, shell_transcript}.rs`
- See [testing.md](testing.md) for detailed test patterns.
