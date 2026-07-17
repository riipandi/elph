# Elph — OpenWiki Quickstart

**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, WASM extensions, and an iocraft-based interactive TUI.

## Repository overview

| Layer       | Crate               | Purpose                                                                                                                    |
| ----------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| **Binary**  | `elph/`             | Coding agent CLI + TUI — product shell                                                                                     |
| **Runtime** | `crates/elph-agent` | App-agnostic agent runtime: turn loop, session persistence, compaction, goals, subagents, skills, MCP client, WASM plugins |
| **AI**      | `crates/elph-ai`    | Unified LLM provider layer: model catalog, provider abstraction, OAuth, image generation, web tools                        |
| **Core**    | `crates/elph-core`  | Shared primitives: `floppy` memory store (Turso vector DB), logger, path resolution, filesystem helpers                    |
| **Exec**    | `crates/elph-exec`  | PTY-based shell execution with configurable timeout, abort, streaming output, and sanitization                             |
| **TUI**     | `crates/elph-tui`   | iocraft component library + examples; primary TUI in `elph` binary (`tui.rs`)                                              |
| **Swarm**   | `crates/elph-swarm` | Multi-agent coordination (early stage)                                                                                     |

## Key concepts

- **Agent** — `elph::agent` wraps `elph-agent`'s `AgentHarness` with session orchestration for the coding use case.
- **AgentHarness** — Stateful, session-backed agent runner with hooks, compaction, branching, and plan mode.
- **Agent Loop** — Low-level turn runner: stream completion → tool call → result → repeat until model stops.
- **Session** — Tree-structured persistence (filesystem or Turso). Sessions can fork, branch, and resume.
- **Compaction** — Automatic context window management via summarization and branch clipping.
- **Goals** — Persisted session objectives with auto-steering.
- **Subagents** — Codex-style multi-agent orchestration (spawn, control, merge).
- **MCP** — Model Context Protocol client supporting stdio, streamable HTTP, and SSE transports with OAuth 2.1 and AES-256-GCM credential encryption.
- **Skills** — Reusable `SKILL.md` files following the [agentskills.io](https://agentskills.io) spec.
- **TOON Encoding** — Optional structured-data encoding for tool results (reduces token usage on tabular payloads).
- **Extensions** — WASM-based dynamic plugins compiled with `wasmtime`.

## Project state (HEAD `227c389`)

This repository is under **active development**. Recent milestones:

- **Model selector** — Multi-tab catalog picker (All / Scoped / Provider) with fuzzy filtering and weighted scoring, rendered as an inline dialog above the status row (`b127f6c`).
- **@-mention file picker** — Inline fuzzy file picker triggered by `@` in the prompt editor; searches workspace via `fff-search` with keyboard navigation and path insertion (`7a0ab91`).
- **Inline dialogs** — Full-width inline dialog pattern shared by tool approval, model picker, and user questions. Structured tool-parameter previews with priority-key highlighting (`594c5c8`, `3e6763a`).
- **Transcript timestamps** — Right-rail `duration + HH:MM` label on user input cards, dimmed to avoid visual clutter (`46b1990`).
- **GFM table rendering** — Box-drawing char table grid with proper column-width measurement in markdown output (`299339f`).
- **Deferred MCP loading** — Agent session starts immediately; MCP tool discovery runs in background with per-server progress/error rows in the transcript (`6e3e0d3`).
- **Ephemeral notices** — Keyed upsert mechanism for transient transcript messages (e.g. agent mode changes) with automatic TTL expiry (`0188ecf`).
- **elph-exec crate** — Dedicated PTY-based shell execution extracted to a separate crate with configurable timeout, abort token, streaming output callbacks, and output sanitization (`d4e86c2`).
- **MCP compat layer** — Normalizes editor-style JSON configs (Cursor, VS Code, Claude Code) via `mcpServers`→`servers` renaming and type inference (`b127f6c`).
- **Dialog shell + theme system** — elph-tui adds `dialog_shell/`, `input_prefix`, `slash_palette/`, `status_indicator`, and `theme` component modules (`9c03b90`).
- **elph-tui growth** — 20+ component modules, 10+ crate-level modules, 26+ examples, 14 integration tests, plus `dialog_shell`, `markdown/`, and `textarea/` sub-directories with sub-modules.

## Documentation map

| Page                                                 | What it covers                                                           |
| ---------------------------------------------------- | ------------------------------------------------------------------------ |
| [quickstart.md](quickstart.md)                       | This page — overview and navigation                                      |
| [architecture/overview.md](architecture/overview.md) | Crate architecture, module map, design principles                        |
| [agent-runtime.md](agent-runtime.md)                 | Agent harness, sessions, turn loop, compaction, goals, subagents, skills |
| [ai-providers.md](ai-providers.md)                   | Model catalog, provider APIs, auth/OAuth, image generation               |
| [mcp-integration.md](mcp-integration.md)             | MCP client, transports, OAuth, encryption, validation, policy            |
| [tui-shell.md](tui-shell.md)                         | iocraft TUI, elph-tui components, slash commands, prompt encoding        |
| [operations.md](operations.md)                       | CLI commands, settings, paths, CI/CD, publishing                         |
| [testing.md](testing.md)                             | Test structure, key patterns, running tests                              |

## Source reading order

For new contributors or future agents:

1. **Cargo.toml** (`/Cargo.toml`) — workspace manifest, dependency versions
2. **elph/src/main.rs** — binary entrypoint
3. **elph/src/cli/mod.rs** — CLI subcommand definitions
4. **crates/elph-agent/src/lib.rs** — agent runtime public API surface
5. **crates/elph-ai/src/lib.rs** — AI provider layer public API
6. **crates/elph-tui/src/lib.rs** — TUI component library (20+ component modules, dialog_shell, slash_palette); examples in `crates/elph-tui/examples/`
7. **crates/elph-core/src/lib.rs** — core library surface

## Quick build & test

```sh
# Prerequisites: Rust >= 1.97
make prepare          # install toolchain, setup hooks
make check            # cargo check --workspace
make test             # cargo nextest run
make build            # release build elph binary
make run              # cargo run --bin elph
make lint             # cargo clippy --workspace -D warnings
```

See [operations.md](operations.md) for CI/CD and publishing, [testing.md](testing.md) for test patterns.

## Design docs

Product design specs live in [`docs/`](/docs/README.md). The `docs/` folder holds _what_ Elph should do; this wiki holds _how_ it works today.
