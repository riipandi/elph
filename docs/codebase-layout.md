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

| Crate / binary | Layout intent                                                                            |
| -------------- | ---------------------------------------------------------------------------------------- |
| `elph-agent`   | Runtime engine: `agent_loop/`, `harness/`, `session/`, `goals/`, `subagent/`, `plugins/` |
| `elph-ai`      | Provider layer: `api/`, `auth/`, `models/`, `providers/`, `utils/`                       |
| `elph-core`    | Shared primitives: `floppy/` (`query/`, `store/`), `logger/`, `scaffold/`, `utils/`      |
| `elph-tui`     | Reusable widgets: `diff/`, `prompt/`, `chrome/`, `shell/`                                |
| `elph`         | Product shell: `agent/`, `shell/`, `cli/`, `platform/`, `extensions/`                    |

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

Top-level `agent_loop/` is the low-level turn runner (stream → tool execution → repeat). The harness wraps it with session persistence, hooks, and compaction.

```
crates/elph-agent/src/
├── agent_loop/              # Core agent turn loop (tools.rs + private run_loop)
├── harness/
│   ├── mod.rs
│   ├── types/               # Error, event, option types (split submodules)
│   ├── hooks.rs
│   ├── agent_harness/
│   │   ├── mod.rs           # AgentHarness struct + core impl
│   │   ├── helpers.rs       # Message builders, validation
│   │   ├── plan_mode.rs
│   │   ├── prompt_ops.rs
│   │   ├── compaction_ops.rs
│   │   ├── tree_nav.rs
│   │   └── run_loop/        # Harness turn loop (split by concern)
│   │       ├── mod.rs       # abort, run entrypoints
│   │       ├── turn_execution.rs
│   │       ├── event_handling.rs
│   │       └── session_writes.rs
│   └── utils/
└── session/, goals/, subagent/, plugins/, …

crates/elph-agent/tests/     # integration tests; shared helpers in tests/common/
```

Extension WASM loading is in `elph-agent/src/plugins/`; `elph/extensions/` wires registry into slash dispatch and `elph plugin`.

## `elph-core` floppy layout

```
crates/elph-core/src/floppy/
├── mod.rs
├── query.rs                 # Task start, memory search, retrieval SQL
├── store/                   # Turso-backed MemoryStore (split submodules)
│   ├── mod.rs
│   ├── read.rs
│   ├── write.rs
│   ├── embed.rs
│   └── tasks.rs
├── scoring.rs, migrations.rs, builder.rs, …
└── (unit tests colocated in src/; no integration tests/ yet)
```

## Crate boundaries

| Crate        | Responsibility                                                                   |
| ------------ | -------------------------------------------------------------------------------- |
| `elph-agent` | AgentHarness, tools, goals, subagents, MCP, **WASM extension host** (`plugins/`) |
| `elph-ai`    | LLM providers, streaming                                                         |
| `elph-tui`   | Reusable TUI components, chrome, diff engine                                     |
| `elph`       | Product binary: CLI + shell + platform glue                                      |

## Test placement rules

| Kind                | Location                    | Examples                                                                              |
| ------------------- | --------------------------- | ------------------------------------------------------------------------------------- |
| Unit                | `#[cfg(test)]` in same file | `paths.rs` path helpers, `settings` merge                                             |
| Integration         | `<crate>/tests/*.rs`        | `elph-agent` harness, `elph-tui` keys, `elph` CLI                                     |
| App integration     | `elph/tests/*.rs`           | CLI `--help`, bootstrap dirs, SIGINT channel                                          |
| Shared test helpers | `<crate>/tests/common/`     | `elph-agent/tests/common/`, `elph-ai/tests/common/` (`mod common;` in each test file) |

Each crate's integration tests exercise that crate's public API. `elph/tests/` covers only the `elph` binary and library glue (`cli.rs`, `bootstrap.rs`, `sigint.rs`).

`elph-core` and `elph-swarm` currently have no `tests/` directory; coverage lives in `#[cfg(test)]` modules next to the code under `src/`.

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
