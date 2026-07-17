# Attribution and Licensing

## Elph workspace licenses

This repository uses a mixed license model:

| Component       | Crates                                                      | License                                |
| --------------- | ----------------------------------------------------------- | -------------------------------------- |
| **Application** | `elph`                                                      | [Apache License 2.0](./LICENSE-APACHE) |
| **Libraries**   | `floppy`, `elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm` | [MIT License](./LICENSE-MIT)           |

When distributing binaries built from `elph`, include the Apache 2.0 license and this notice file.
When using or redistributing the library crates, include the MIT license and retain upstream attributions below.

---

## Third-party attributions

Elph is a Rust workspace for building AI agent applications. It was originally inspired by
[pi](https://github.com/earendil-works/pi) by [Mario Zechner](https://github.com/badlogicgames),
licensed under the MIT License.

The original pi project is a TypeScript-based AI agent toolkit providing a unified
multi-provider LLM API, agent runtime with tool calling and state management,
terminal UI library with differential rendering, and an interactive coding agent CLI.

Elph re-implements these concepts in Rust with the following key differences:

- **Language**: Rust (edition 2024) instead of TypeScript
- **Async runtime**: Tokio instead of Node.js
- **Rendering**: `iocraft` + `crossterm` instead of pi-tui
- **Serialization**: Serde + JSONL instead of JSON
- **Memory store**: Turso-backed vector embeddings for lifelong agent context
- **Codegraph**: `elph codegraph` — AST-graph analysis for structural code review
- **MCP**: Built-in Model Context Protocol client integration
- **Subagents**: Built-in sub-agent orchestration (spawn, steer, resume)
- **Agent Swarm**: Multi-agent swarm coordination via `elph-swarm` crate
- **Extension system (planned)**: WASM instead of npm packages

The architectural design — tool system, provider abstraction, streaming event model,
session tree structure, and overall API shape — is derived from the original pi project.
All original pi code is Copyright (c) 2025 Mario Zechner, used under the MIT License.

Portions of the agent workflow (exit summary, goals, subagent orchestration, tool transcript layout)
are inspired by [OpenAI Codex CLI](https://github.com/openai/codex).
The original Codex CLI is Copyright (c) 2025 OpenAI, licensed under the Apache License 2.0.

The `floppy` crate is a Rust port of the [memelord SDK](https://github.com/glommer/memelord/tree/main/packages/sdk).
The original memelord code is Copyright (c) 2026 Glauber Costa, used under the MIT License.

The `elph codegraph` integrates [code-review-graph](https://github.com/tirth8205/code-review-graph)
by Tirth Kanani. The original code-review-graph is Copyright (c) 2026 Tirth Kanani, used under the MIT License.

The Elph coding-agent system prompt (`elph/templates/agent/coding_base.md` and related mode templates)
adapts section conventions from [Grok Build](https://github.com/xai-org/grok-build) by SpaceXAI — including
tagged blocks such as `<action_safety>`, `<tool_calling>`, `<output_efficiency>`, and `<formatting>` —
integrated with the Pi/elph-agent `SystemPromptBuilder` assembly pipeline (generic base template, Pi-style
`<project_context>`, and elph-agent tool/skills context). Grok Build is Copyright (c) 2023–2026 SpaceXAI,
licensed under the Apache License 2.0.
