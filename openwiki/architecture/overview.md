---
type: Architecture
title: Elph Architecture Overview
description: High-level architecture, crate dependency graph, and design principles for the Elph AI Agent workspace.
tags: [architecture, design, rust, workspace]
---

# Architecture Overview

## Crate dependency graph

```
                    ┌───────────────────────────────────┐
                    │         elph (binary)             │
                    │  CLI + TUI + coding agent wiring  │
                    └────┬───────┬───────┬───────┬──────┘
                         │       │       │       │
              ┌──────────┤       │       │       │
              │          │       │       │       │
        ┌─────▼────┐ ┌───▼───┐ ┌─▼─────┐ │ ┌─────▼──────┐
        │ elph-tui │ │elph-ai│ │elph-  │ │ │ elph-exec  │
        │(iocraft) │ │(LLM)  │ │agent  │ │ │ (PTY/shell)│
        └──────────┘ └───┬───┘ └─┬─────┘ │ └────────────┘
                         │       │       │
                   ┌─────▼───────▼───────▼─┐
                   │     elph-core         │
                   │ (fs, logger, utils,   │
                   │  trace, scaffold,     │
                   │  floppy memory)       │
                   └───────────────────────┘
```

**Source:** `/Cargo.toml` workspace members, each crate's `Cargo.toml` dependency declarations.

## Layer responsibilities

### Binary layer: `elph/`

The `elph` package (`/elph/Cargo.toml`) produces both a library and binary. `main.rs` parses CLI args with clap and dispatches to `cli::run()`. The library crate exposes all modules for integration testing.

Key routing:

- **Interactive TUI** → `tui::run_tui()` — iocraft-based shell
- **Non-interactive** → `cli::run::run()` — agent run in non-interactive mode
- **Admin subcommands** → `cli::mcp`, `cli::memory`, `cli::session`, etc.

### Agent runtime: `elph-agent`

Generic, app-agnostic agent runtime (`/crates/elph-agent/`). See [agent-runtime.md](../agent-runtime.md).

Key modules:

| Module           | Responsibility                                             |
| ---------------- | ---------------------------------------------------------- |
| `runtime/`       | Turn loop: stream → tool execute → repeat                  |
| `agent/harness/` | Agent harness: session persistence, hooks, compaction      |
| `tools/`         | Built-in tools (edit, search, web, shell_exec, MCP)        |
| `tools/mcp/`     | MCP client: stdio, HTTP, SSE; OAuth; policy engine         |
| `session/`       | Session storage (in-memory, dir, Turso)                    |
| `compaction/`    | History compaction, token estimation, branch summarization |
| `goals/`         | Goal/todo lifecycle with budgets                           |
| `messages/`      | Message types, shell output formatting                     |
| `collaboration/` | Plan/implement modes with tool exposure policies           |
| `plugins/`       | WASM extension support via wasmtime                        |

### LLM provider layer: `elph-ai`

Provider-agnostic LLM API (`/crates/elph-ai/`). Supports 10+ providers.

| Module       | Responsibility                                          |
| ------------ | ------------------------------------------------------- |
| `api/`       | Provider API implementations                            |
| `auth/`      | API key resolution, OAuth 2.1 + PKCE                    |
| `models/`    | Model catalog with capabilities metadata                |
| `providers/` | Provider definitions + `faux` mock provider for testing |

### Core primitives: `elph-core`

Shared foundational utilities (`/crates/elph-core/`).

| Module        | Responsibility                                      |
| ------------- | --------------------------------------------------- |
| `fs/`         | File helpers: ensure_dirs, write_json/private files |
| `logger/`     | Logging config (logforth + fastrace integration)    |
| `trace/`      | Distributed tracing spans (fastrace)                |
| `scaffold/`   | Project initialization, trust store, version file   |
| `utils/git/`  | Git integration (git2)                              |
| `utils/path/` | XDG path resolution, AppPaths, PathResolver         |
| `floppy/`     | Vector memory with Turso + ONNX embeddings          |

### TUI: `elph-tui`

Reusable terminal UI components built on the patched `iocraft` crate (`/crates/elph-tui/`). See [tui-shell.md](../tui-shell.md).

## Design principles

1. **Minimal agent CLI** — one interactive binary, non-interactive `run`, and admin subcommands
2. **Native tool calling** — models invoke tools via provider APIs; text markup is fallback only
3. **Durable sessions** — conversations, checkpoints, metadata survive restarts
4. **Project memory** — cross-session lessons via vector store; semantic retrieval at task start
5. **Light TUI** — multiline prompt, sticky-tail scroll, inline tools, minimal chrome
6. **Safe defaults** — risky tools require approval; _brave_ mode is opt-in
7. **Feature-gated tools** — built-in tools are Cargo features for minimal builds
8. **No direct env reading in libraries** — host binary passes config explicitly
9. **Deep merge settings** — project overlays per nested key, never written back to home

## License split

- **Application** (`elph/`) — Apache 2.0
- **Libraries** — MIT (elph-core, elph-ai, elph-agent, elph-tui, elph-swarm)

This is intentional: users deploying the binary get strong patent protections via Apache 2.0; library consumers get permissive MIT.

## Key design decisions from git history

1. **Rust edition 2024** ([commit `901dd3c`](https://github.com/riipandi/elph/commit/901dd3c)) — refactored `elph-core` into floppy, elph, and library crates. Dissolved the intermediate `elph-core` crate and redistributed its content.
2. **elph-core reintroduction** ([commit `ed330a0`](https://github.com/riipandi/elph/commit/ed330a0)) — reverted the dissolve when the split created import complexity; consolidated again as a shared primitives crate.
3. **TOON encoding** — compressed JSON format for tool results to reduce token consumption; controlled by `ELPH_PROMPT_ENCODING` env var.
4. **Patched iocraft** — local vendor at `/vendor/iocraft/` for bracketed paste support not yet on crates.io.
5. **`shell_exec` rename** ([commit `f014ddd`](https://github.com/riipandi/elph/commit/f014ddd)) — `bash` tool renamed to `shell_exec` across the codebase for platform-neutral naming.
