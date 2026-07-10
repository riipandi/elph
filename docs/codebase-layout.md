# Codebase layout

Design for how workspace crates are organized — separation of concerns, test placement, file-size limits, and scaling rules.

Implementation detail lives in [openwiki](../openwiki/quickstart.md); this document defines the **intended** module map.

## Principles

1. **Pi coding-agent port** — `agent/` owns session orchestration above `elph-agent`; not mixed with CLI or TUI chrome.
2. **Thin binary** — `main.rs` only parses CLI and exits; library crate holds modules for tests.
3. **Platform vs product** — `platform/` is paths, settings, bootstrap, datastore; no agent logic.
4. **Shell vs agent** — `shell/` is the interactive TUI app; `agent/` is the coding session runtime.
5. **Tests** — unit tests colocated with the code they cover; integration tests only under `elph/tests/`.
6. **File size** — prefer modules under ~400 lines; split by concern (not by arbitrary line count). Free functions and wiring logic extract to sibling files; keep a single `impl` block per type when methods call each other privately.

## Workspace crates

| Crate / binary | Layout intent |
| -------------- | ------------- |
| `elph`         | Product shell: `agent/`, `shell/`, `cli/`, `platform/`, `extensions/` |
| `elph-agent`   | Runtime engine: `harness/`, `agent_loop/`, `session/`, `goals/`, `subagent/`, `plugins/` |
| `elph-ai`      | Provider layer: `api/`, `auth/`, `models/`, `providers/`, `utils/` |
| `elph-core`    | Shared primitives: `floppy/`, `logger/`, `scaffold/`, `utils/` |
| `elph-tui`     | Reusable widgets: `diff/`, `prompt/`, `chrome/`, `shell/` |
| `owly`         | Docs agent binary (own `src/` tree; tests in `owly/tests/`) |
| `eclaw`        | Release tooling (minimal `src/main.rs`) |

Crate-level integration tests stay in `crates/<crate>/tests/`; only the `elph` **application** integration tests live in `elph/tests/`.

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
│   │   ├── mod.rs           # Cli struct, dispatch
│   │   ├── run.rs, plugin.rs, memory.rs, …
│   │   └── default.rs       # No subcommand → platform::run → shell
│   │
│   ├── platform/            # Host environment (was `runtime/`)
│   │   ├── paths.rs         # ~/.elph, project .elph/
│   │   ├── settings.rs
│   │   ├── bootstrap.rs
│   │   ├── datastore/
│   │   ├── migrations.rs
│   │   └── app.rs           # Exit codes, run() wrapper
│   │
│   ├── extensions/          # Extension host wiring (CLI side)
│   │   └── mod.rs           # ExtensionHost: load, reload, slash dispatch
│   │
│   ├── tui/                 # Transcript bridge (elph-tui adapters)
│   ├── memory/              # Floppy memory CLI backing
│   ├── skills/, prompt/, widget/, worktree/, config/, command/
│   │
│   └── (no business logic in root)
│
└── tests/                   # Integration tests only
    ├── cli.rs
    ├── bootstrap.rs
    ├── sigint.rs
    └── …
```

## `elph-agent` harness layout

```
crates/elph-agent/src/harness/
├── mod.rs
├── types.rs              # Harness error/event/option types (split when >1k lines)
├── hooks.rs
├── agent_harness/
│   ├── mod.rs            # AgentHarness struct + impl (session loop)
│   └── helpers.rs        # Message builders, validation, NavigateTreeOptions
└── utils/
```

Extension WASM loading is implemented in `elph-agent/src/plugins/`; `elph/extensions/` only wires registry into slash dispatch and `elph plugin`.

## Crate boundaries

| Crate        | Responsibility                                                                   |
| ------------ | -------------------------------------------------------------------------------- |
| `elph-agent` | AgentHarness, tools, goals, subagents, MCP, **WASM extension host** (`plugins/`) |
| `elph-ai`    | LLM providers, streaming                                                         |
| `elph-tui`   | Reusable TUI components, chrome, diff engine                                     |
| `elph`       | Product binary: CLI + shell + platform glue                                      |

## Test placement rules

| Kind              | Location                    | Examples                                     |
| ----------------- | --------------------------- | -------------------------------------------- |
| Unit              | `#[cfg(test)]` in same file | `paths.rs` path helpers, `settings` merge    |
| Integration       | `elph/tests/*.rs`           | CLI `--help`, bootstrap dirs, SIGINT channel |
| Crate integration | `crates/*/tests/`           | `elph-agent` harness, goals, plugins         |

Integration tests may use `elph` as a library (`elph::platform::…`) or subprocess (`CARGO_BIN_EXE_elph`).

## Naming conventions

| Old name        | New name       | Rationale                                       |
| --------------- | -------------- | ----------------------------------------------- |
| `coding_agent/` | `agent/`       | Shorter; matches Pi "coding agent" product term |
| `cmd/`          | `cli/`         | Matches Rust ecosystem (`clap`, subcommands)    |
| `runtime/`      | `platform/`    | Avoid confusion with `elph-agent` runtime       |
| `app.rs` (root) | `shell/app/`   | TUI shell split into focused submodules         |

## Related

- [extensions.md](./extensions.md)
- [agent-runtime.md](./agent-runtime.md)
- [cli.md](./cli.md)
