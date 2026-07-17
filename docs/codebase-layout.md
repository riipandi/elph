# Codebase layout

Design for how workspace crates are organized — separation of concerns, test placement, file-size limits, and scaling rules.

Implementation detail lives in [openwiki](../openwiki/quickstart.md); this document defines the **intended** module map.

## Principles

1. **Pi coding-agent port** — `agent/` owns session orchestration above `elph-agent`; not mixed with CLI or TUI chrome.
2. **Thin binary** — `main.rs` only parses CLI and exits; library crate holds modules for tests.
3. **Platform vs product** — `platform/` is paths, settings, bootstrap, datastore; no agent logic.
4. **Shell vs agent** — `shell/` is the interactive TUI app; `agent/` is the coding session runtime.
5. **Tests** — unit tests colocated with the code they cover; integration tests in each crate's `tests/` directory.
6. **File size** — prefer modules under ~400 lines; split by concern (not by arbitrary line count). Free functions and wiring logic extract to sibling files; use `pub(super)` when splitting `impl` blocks across files in the same module.

## Workspace crates

| Crate / binary | Layout intent                                                                                   |
| -------------- | ----------------------------------------------------------------------------------------------- |
| `elph-agent`   | Generic runtime: harness, agent loop, optional MCP/skills, `SystemPromptBuilder` (MiniJinja)    |
| `elph-ai`      | Provider layer: `api/`, `auth/`, `models/`, `providers/`, `utils/`, optional fastrace `trace/`  |
| `floppy`       | Standalone AI memory: Turso vector search, scoring, task lifecycle (`query/`, `store/`)         |
| `elph-tui`     | Reusable widgets: `diff/`, `prompt/`, `chrome/`, `shell/`                                       |
| `elph`         | Coding agent product: `agent/`, `logger/`, `scaffold/`, `utils/`, `shell/`, `cli/`, `platform/` |

## `elph` module map

```
elph/
├── src/
│   ├── main.rs              # Entry: clap → cli::run
│   ├── lib.rs               # Public modules (for integration tests)
│   │
│   ├── agent/               # Pi coding-agent equivalent
│   │   ├── runtime.rs       # CreateSessionOptions, harness wiring
│   │   ├── session/         # CodingAgentSession
│   │   │   ├── mod.rs       # Public session API
│   │   │   └── wiring.rs    # Harness → UI event bridge
│   │   ├── session_manager.rs
│   │   ├── slash_commands.rs
│   │   ├── goal_slash.rs
│   │   ├── tool_policy.rs
│   │   ├── run_mode.rs      # Non-interactive `elph run`
│   │   └── …
│   │
│   ├── shell/               # Interactive TUI application
│   │   └── app/             # ElphApp (split by concern)
│   │       ├── mod.rs       # State, bootstrap
│   │       ├── overlays.rs  # Model/session/tree selectors
│   │       ├── events.rs    # UI event poll, global keys
│   │       ├── slash.rs     # Slash command dispatch
│   │       ├── turn.rs      # Turn / queue lifecycle
│   │       ├── input.rs     # Prompt + modal input
│   │       └── render.rs    # Frame render, run_tui, SIGINT
│   │
│   ├── cli/                 # Subcommands (was `cmd/`)
│   ├── platform/            # Host environment (was `runtime/`)
│   ├── extensions/          # Extension host wiring (CLI side)
│   ├── tui/, memory/, skills/, prompt/, widget/, worktree/
│   └── (no business logic in root)
│
└── tests/                   # elph application integration tests only
    ├── cli.rs
    ├── bootstrap.rs
    └── sigint.rs
```

## `elph-agent` module layout

Top-level `runtime/` is the agent turn runner (stream → tool execution → repeat) plus env, proxy, and async bridge. The agent harness wraps it with session persistence, hooks, and compaction.

```
crates/elph-agent/src/
├── builder.rs               # AgentBuilder (logging) + BuiltinToolsBuilder (tool catalog) + InitProgress
├── agent/
│   ├── mod.rs               # Agent struct + events, queue, run, state
│   ├── harness/             # AgentHarness (flattened from harness/agent_harness)
│   │   ├── mod.rs           # AgentHarness struct + core impl + re-exports
│   │   ├── helpers.rs       # Message builders, validation
│   │   ├── plan_mode.rs, prompt_ops.rs, compaction_ops.rs, tree_nav.rs
│   │   ├── run_loop/        # Harness turn loop (split by concern)
│   │   ├── types/           # Error, event, option types
│   │   ├── utils/           # Truncation, shell output
│   │   └── hooks.rs, system_prompt.rs, generic_on.rs
│   └── subagent/            # Multi-agent orchestration
├── runtime/
│   ├── mod.rs               # agent_loop entry + block_on/try_block_on
│   ├── loop_config.rs       # AgentLoopConfig, AgentContext, AgentEvent, callbacks
│   ├── run_loop.rs          # Core turn iteration
│   ├── stream.rs            # Assistant response streaming
│   ├── event_stream.rs      # AgentEventStream + AgentEventSink
│   ├── exec/                # Tool execution pipeline (was runtime/tools)
│   ├── env.rs               # Path helpers (LocalExecutionEnv in local_env/)
│   ├── local_env/           # Filesystem + shell execution
│   └── proxy.rs             # Browser stream proxy
├── tools/
│   ├── types.rs             # AgentTool, AgentToolResult, ToolResultContent
│   ├── mcp/                 # Model Context Protocol clients
│   ├── web/                 # Web fetch + search tools
│   └── shell_exec.rs, read.rs, write.rs, grep.rs, …
├── types/
│   └── enums.rs             # Global enums (QueueMode, ToolExecutionMode, AgentThinkingLevel)
├── collaboration/           # Collaboration modes (Plan / Default) + tool policy
├── messages/
│   ├── types.rs             # AgentMessage, CustomAgentMessage
│   └── mod.rs               # Message bridge (convert_to_llm, custom roles)
├── prompt/
│   └── encoding/            # TOON encoding for structured prompt payloads
├── compaction/, session/, goals/, skills/, plugins/, datastore/, trace/
└── lib.rs                   # Crate root (module declarations + re-exports)

## `floppy` layout

```

crates/floppy/src/
├── lib.rs
├── query/ # Task start, memory search, retrieval SQL
├── store/ # Turso-backed MemoryStore (split submodules)
│ ├── mod.rs
│ ├── read.rs
│ ├── write.rs
│ ├── embed.rs
│ └── tasks.rs
├── scoring.rs, migrations.rs, builder.rs, …
└── (unit tests colocated in src/)

```

## Crate boundaries

| Crate        | Responsibility                                                                                                       |
| ------------ | -------------------------------------------------------------------------------------------------------------------- |
| `elph-agent` | Agent + AgentHarness, optional built-in tools (`builtin-tools`), goals, subagents, MCP (`tools/mcp/`), **WASM extension host** (`plugins/`) |
| `elph-ai`    | LLM providers, streaming, optional fastrace reporter                                                                 |
| `floppy`     | Standalone AI memory store (Turso vector search)                                                                     |
| `elph-tui`   | Reusable TUI components, chrome, diff engine                                                                         |
| `elph`       | Product binary: CLI + shell + platform glue + logger/scaffold                                                        |

## Test placement rules

| Kind                | Location                    | Examples                                                                              |
| ------------------- | --------------------------- | ------------------------------------------------------------------------------------- |
| Unit                | `#[cfg(test)]` in same file | `paths.rs` path helpers, `settings` merge                                             |
| Integration         | `<crate>/tests/*.rs`        | `elph-agent` harness, `elph-tui` keys, `elph` CLI                                     |
| App integration     | `elph/tests/*.rs`           | CLI `--help`, bootstrap dirs, SIGINT channel                                          |
| Shared test helpers | `<crate>/tests/common/`     | `elph-agent/tests/common/`, `elph-ai/tests/common/` (`mod common;` in each test file) |

Each crate's integration tests exercise that crate's public API. `elph/tests/` covers only the `elph` binary and library glue (`cli.rs`, `bootstrap.rs`, `sigint.rs`).

`floppy` and `elph-swarm` currently have no top-level `tests/` directory; coverage lives in `#[cfg(test)]` modules next to the code under `src/`.

## Naming conventions

| Old name        | New name     | Rationale                                       |
| --------------- | ------------ | ----------------------------------------------- |
| `coding_agent/` | `agent/`     | Shorter; matches Pi "coding agent" product term |
| `cmd/`          | `cli/`       | Matches Rust ecosystem (`clap`, subcommands)    |
| `runtime/`      | `platform/`  | Avoid confusion with `elph-agent` runtime       |
| `app.rs` (root) | `shell/app/` | TUI shell split into focused submodules         |

## Related

- [extensions.md](./extensions.md)
- [agent-runtime.md](./agent-runtime.md)
- [cli.md](./cli.md)
```
