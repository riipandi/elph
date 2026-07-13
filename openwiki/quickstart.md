# Elph — OpenWiki Quickstart

**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, WASM extensions, and a tuie-based interactive TUI.

## Repository overview

| Layer       | Crate               | Purpose                                                                                                                    |
| ----------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| **Binary**  | `elph/`             | Coding agent CLI + TUI — product shell                                                                                     |
| **Runtime** | `crates/elph-agent` | App-agnostic agent runtime: turn loop, session persistence, compaction, goals, subagents, skills, MCP client, WASM plugins |
| **AI**      | `crates/elph-ai`    | Unified LLM provider layer: model catalog, provider abstraction, OAuth, image generation, web tools                        |
| **Core**    | `crates/elph-core`  | Shared primitives: `floppy` memory store (Turso vector DB), logger, path resolution, filesystem helpers                    |
| **TUI**     | `crates/elph-tui`   | Reusable terminal UI widgets built on [tuie](https://crates.io/crates/tuie)                                                |
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

## Project state (HEAD `e90514a`)

This repository is under **active development**. Recent milestones:

- **Refactored** from a monolithic layout to layered crates (`elph-agent`, `elph-ai`, `elph-core`, `elph-tui`, `elph-swarm`).
- **MCP** — Full client integration: stdio, streamable HTTP, SSE, OAuth, encrypted credentials, tool policy, session pools, hot reload.
- **TUI** — Migrated from `superlighttui` to `tuie` framework (commit `b06c134`).
- **Prompt encoding** — Optional TOON encoding for tool results (`0a0753c`).
- **Auto session naming** — Model-generated thread titles (`2e0297f`).
- **Sandbox policies** — Zerobox-powered sandboxed tool execution (early stage).

## Documentation map

| Page                                                 | What it covers                                                           |
| ---------------------------------------------------- | ------------------------------------------------------------------------ |
| [quickstart.md](quickstart.md)                       | This page — overview and navigation                                      |
| [architecture/overview.md](architecture/overview.md) | Crate architecture, module map, design principles                        |
| [agent-runtime.md](agent-runtime.md)                 | Agent harness, sessions, turn loop, compaction, goals, subagents, skills |
| [ai-providers.md](ai-providers.md)                   | Model catalog, provider APIs, auth/OAuth, image generation               |
| [mcp-integration.md](mcp-integration.md)             | MCP client, transports, OAuth, encryption, validation, policy            |
| [tui-shell.md](tui-shell.md)                         | tuie widgets, elph shell app, slash commands, prompt encoding            |
| [operations.md](operations.md)                       | CLI commands, settings, paths, CI/CD, publishing                         |
| [testing.md](testing.md)                             | Test structure, key patterns, running tests                              |

## Source reading order

For new contributors or future agents:

1. **Cargo.toml** (`/Cargo.toml`) — workspace manifest, dependency versions
2. **elph/src/main.rs** — binary entrypoint
3. **elph/src/cli/mod.rs** — CLI subcommand definitions
4. **crates/elph-agent/src/lib.rs** — agent runtime public API surface
5. **crates/elph-ai/src/lib.rs** — AI provider layer public API
6. **crates/elph-tui/src/lib.rs** — TUI widget public API
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
