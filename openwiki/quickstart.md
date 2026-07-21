---
type: Overview
title: Elph Quickstart
description: Entrypoint for the Elph AI Agent workspace — Rust workspace with a coding agent CLI/TUI and shared agent runtime libraries.
tags: [elph, ai-agent, rust, workspace, quickstart]
resource: /
---

# Elph AI Agent Workspace

**Elph** is a Rust workspace for AI agent applications — a coding agent CLI and TUI, plus shared runtime libraries and terminal UI components. It re-implements concepts from [pi](https://pi.dev), [OpenAI Codex CLI](https://github.com/openai/codex), and [memelord](https://github.com/glommer/memelord) in Rust.

## Quick overview

| Component          | Location              | Description                                                                                      |
| ------------------ | --------------------- | ------------------------------------------------------------------------------------------------ |
| Binary CLI + TUI   | `/elph/`              | The `elph` application — interactive TUI, non-interactive `run`, admin subcommands               |
| Agent runtime      | `/crates/elph-agent/` | App-agnostic agent harness: turn loop, tool execution, MCP, sessions, subagents, compaction      |
| LLM provider layer | `/crates/elph-ai/`    | Provider-agnostic LLM API: OpenAI-compatible, Anthropic, Bedrock, Gemini, Copilot, Mistral, etc. |
| Core primitives    | `/crates/elph-core/`  | Shared utilities: `floppy` vector memory, logger, scaffold, git, path resolution, tracing        |
| TUI components     | `/crates/elph-tui/`   | Reusable iocraft-based widgets: markdown, textarea, themes, transcript layout                    |
| Shell execution    | `/crates/elph-exec/`  | Configurable local shell and PTY execution                                                       |

### Placeholder crates (not yet implemented)

- `elph-cron` — cron-based scheduled tasks (empty)
- `elph-sandbox` — sandbox execution (empty)
- `elph-swarm` — multi-agent swarm orchestration (empty)
- `floppy` — standalone AI memory crate (empty; implementation lives in `elph-core/src/floppy/`)

## Installation

```sh
# Pre-built binary (Linux/macOS)
curl -fsSL https://elph.space/elph/install.sh | bash

# From crates.io
cargo install --locked elph

# From source
cargo install --path elph
```

Requires Rust >= 1.97 (edition 2024).

## Development setup

```sh
git clone https://github.com/riipandi/elph.git
cd elph
make prepare        # install toolchain, tools, and vendor deps
make check          # fast compile check (no codegen)
make test           # cargo nextest run
make lint           # cargo clippy --workspace -D warnings
make run            # cargo run --bin elph
```

## Key concepts

### Agent modes

| Mode      | Description                       | TUI accent |
| --------- | --------------------------------- | ---------- |
| **Build** | Full tool access, productive mode | White      |
| **Plan**  | Read-only, research mode          | Yellow     |
| **Ask**   | Chat mode, no code changes        | Blue       |
| **Brave** | All tools auto-approved           | Orange     |

### Thinking levels

Controls reasoning depth: `Off` → `Minimal` → `Low` → `Medium` → `High` → `Xhigh` → `Max`. Each has a distinct footer color.

### Agent loop

The core agent runtime (`elph-agent`) implements a turn cycle:

1. Assemble system prompt + conversation history + resources
2. Stream completions from the LLM provider (with tool schemas)
3. If tool calls are made → approval (if risky) → execute → append results → repeat
4. When model stops calling tools → persist history → emit turn completion
5. Automatic compaction when limits are reached (~32 messages, ~512KB)

## Documentation map

| Page                                                     | Covers                                                             |
| -------------------------------------------------------- | ------------------------------------------------------------------ |
| [quickstart.md](quickstart.md)                           | Overview, installation, navigation                                 |
| [architecture/overview.md](architecture/overview.md)     | System architecture, crate dependencies, design principles         |
| [architecture/source-map.md](architecture/source-map.md) | Crate-by-crate source map with module paths                        |
| [agent-runtime.md](agent-runtime.md)                     | Agent loop, harness, sessions, tools, subagents, goals, compaction |
| [mcp-integration.md](mcp-integration.md)                 | MCP configuration, auth, tool registry                             |
| [operations.md](operations.md)                           | CLI subcommands, config system, env vars, paths, observability     |
| [tui-shell.md](tui-shell.md)                             | TUI layout, interaction modes, theme system                        |
| [testing.md](testing.md)                                 | Test organization, running tests, per-crate test areas             |

## License

- **Application** (`elph/`) — Apache 2.0
- **Libraries** (`elph-core`, `elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm`) — MIT

## Backlog

- **WASM extensions (phase 2)** — `/elph/src/extensions/`, `/elph/src/cli/extensions.rs` — wasmtime Component Model API beyond slash commands
- **Collaboration protocol** — `/crates/elph-agent/src/collaboration/` — plan/implement mode refinement
- **Codegraph integration** — `/elph/src/cli/codegraph.rs` — structural code review graph (external tool)
- **Prompt templates directory** — `/crates/elph-agent/src/prompt/` — MiniJinja template system (designed, not fully documented)
