# Elph design docs

`docs/` holds **product design** — intended behavior, UX layout, feature contracts, and high-level architecture. It is not implementation documentation.

Living documentation (how the code works today, source maps, usage) lives in **[openwiki/](../openwiki/quickstart.md)**.

## Roles

| Folder      | Contents                               | Audience                  |
| ----------- | -------------------------------------- | ------------------------- |
| `docs/`     | Specifications — _what_ Elph should do | Product, UX, architecture |
| `openwiki/` | Repository docs — _how_ it works now   | Developers, operators     |

When a design is implemented, technical detail belongs in openwiki — not duplicated here.

## Design map

| Topic                 | File                                         |
| --------------------- | -------------------------------------------- |
| Agent flow & sessions | [agent-runtime.md](./agent-runtime.md)       |
| WASM extensions       | [extensions.md](./extensions.md)             |
| `elph` crate layout   | [codebase-layout.md](./codebase-layout.md)   |
| Tool catalog          | [tools.md](./tools.md)                       |
| Configuration & paths | [configuration.md](./configuration.md)       |
| TUI & interaction     | [tui.md](./tui.md)                           |
| Slash commands        | [slash-commands.md](./slash-commands.md)     |
| Prompt templates      | [prompt-templates.md](./prompt-templates.md) |
| CLI surface           | [cli.md](./cli.md)                           |
| Local development     | [development.md](./development.md)           |
| Agent memory          | [memory.md](./memory.md)                     |
| Dependency evaluation | [consideration.md](./consideration.md)       |
| Platform limits       | [limitation.md](./limitation.md)             |

## Design principles

1. **Minimal agent CLI** — one interactive binary, non-interactive `run`, and admin subcommands.
2. **Native tool calling** — models invoke tools via provider APIs; text markup is fallback only.
3. **Durable sessions** — conversations, checkpoints, and metadata survive restarts.
4. **Project memory** — cross-session lessons in the repo; semantic retrieval at task start.
5. **Light TUI** — multiline prompt, sticky-tail scroll, inline tools, minimal chrome.
6. **Safe defaults** — risky tools require approval; _brave_ mode is opt-in.

## Implementation snapshot

Details and source maps: [openwiki/quickstart.md](../openwiki/quickstart.md).

| Area                        | Target      | Notes                                    |
| --------------------------- | ----------- | ---------------------------------------- |
| Agent loop + tools          | Done        | `elph-agent` harness                     |
| Goals + nested subagents    | Done        | Codex-style goals, depth-3 subagents     |
| MCP → agent loop            | Done        | `mcp_{server}__{tool}` registry          |
| WASM extensions (phase 1)   | In progress | Slash commands; wasmtime Component Model |
| `elph` crate layout         | Done        | `agent/`, `cli/`, `platform/`, `shell/`  |
| Owly (agent + TUI)          | Done        | Documentation agent reference            |
| Elph TUI + coding agent     | In progress | Shell wired; overlays partially stubbed  |
| Elph slash commands         | In progress | Built-ins + `/goal`; extension commands  |
| Prompt templates            | Planned     | Format and dirs designed                 |
| Provider / MCP / server CLI | Planned     | Commands defined, incomplete             |
| Memory CLI                  | Done        | Inspect and maintain store               |

## Where to start

- Design: [agent-runtime.md](./agent-runtime.md), [configuration.md](./configuration.md)
- Implementation: [openwiki/architecture.md](../openwiki/architecture.md)
