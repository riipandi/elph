---
type: Workspace
title: Elph Workspace Quickstart
description: Entrypoint for the Elph Rust workspace — workspace layout, make targets, crate dependency graph, and navigation guide
tags: [elph, workspace, quickstart, rust]
---

# Elph Workspace Quickstart

[Elph](https://elph.space) is a Rust workspace (edition 2024, resolver v2, Rust 1.97) at `github.com/riipandi/elph` that builds an AI coding agent CLI, shared runtime libraries, and terminal UI components. It ports core concepts from [pi](https://pi.dev) (TypeScript) by Mario Zechner and adds Elph-only extensions.

## Workspace Layout

```
Cargo.toml          # workspace root — resolver = "2", members = ["crates/elph-*", "crates/floppy", "elph"]
Makefile            # build, test, lint, release, cross-compilation targets
elph/               # product binary + library (CLI, TUI, agent session orchestration)
crates/
├── elph-agent/     # app-agnostic agent runtime (port of @earendil-works/pi-agent)
├── elph-ai/        # unified LLM API with provider collections, auth, streaming (port of @earendil-works/pi-ai)
├── elph-cron/      # skeleton — cron scheduled tasks
├── elph-exec/      # local shell & PTY execution
├── elph-sandbox/   # skeleton — sandbox (zerobox)
├── elph-swarm/     # skeleton — multi-agent orchestration
├── elph-tui/       # reusable iocraft-based TUI components
└── floppy/         # AI memory with vector search (Turso); port of memelord
docs/               # project documentation, porting status, design notes
skills/             # OpenWiki skill files (mermaid-diagrams, migrate-wiki, write-connector)
extensions/         # WASM extension plugins (say-hello)
templates/agent/    # MiniJinja system prompt templates (coding_base.md, mode_*.md, session_title_*.md)
```

## Make Targets

| Target                 | Description                                                            |
| ---------------------- | ---------------------------------------------------------------------- |
| `make build`           | Build `elph` binary (debug; `RELEASE=1` or `-- --release` for release) |
| `make run`             | Run elph coding agent                                                  |
| `make watch`           | Run with hot reload (requires watchexec)                               |
| `make test`            | Run all workspace tests via `cargo nextest`                            |
| `make test-elph`       | Test `elph-ai` + `elph` + `elph-agent` (with `--features full`)        |
| `make test-elph-tui`   | Test `elph-tui`                                                        |
| `make check`           | Check compilation (no codegen)                                         |
| `make lint`            | Run clippy with `-D warnings` (`elph`, `elph-agent`, `elph-ai`)        |
| `make fmt`             | Format all Rust code + models + wiki                                   |
| `make coverage`        | Test coverage via `cargo-llvm-cov`                                     |
| `make generate-models` | Regenerate `elph-ai` model catalogs from pi upstream                   |
| `make cross`           | Cross-compile one platform (`CROSS_TARGET=<triple>`)                   |
| `make cross-pull`      | Pull cross-compilation Docker images                                   |
| `make release`         | Host-aware release build                                               |
| `make install`         | Install to `~/.local/bin/` (debug → `elph-dev`, release → `elph-next`) |
| `make prepare`         | Prepare workspace for development                                      |
| `make clean`           | Clean build artifacts                                                  |
| `make stats`           | Show sccache stats and line counts                                     |
| `make publish`         | Publish crates                                                         |
| `make version`         | Show version info                                                      |

## Crate Dependency Graph

```
elph (binary + lib)
├── elph-agent (--features "tracing, builtin-tools, mcp, prompt-templates, extensions")
│   ├── elph-ai (--features "tracing")
│   ├── elph-exec (shell execution)
│   └── floppy (memory, --features "embed")
├── elph-ai
├── elph-tui (iocraft-based TUI components)
├── floppy
└── git2, iocraft, tokio, clap, etc.
```

The `elph` crate is the product binary. Its `lib.rs` exports modules for:

- `cli/` — 19 subcommands (ACP, codegraph, provider, run, server, session, etc.)
- `agent/` — session orchestration above `elph-agent` (modes, prompts, tools, MCP bootstrap)
- `tui/` — interactive TUI shell powered by `iocraft`
- `platform/` — paths, settings, datastore, bootstrap
- `memory/` — floppy memory hooks, commands, tools
- `extensions/` — WASM extension host

## Key Feature Flags in `elph-agent`

```toml
# crates/elph-agent/Cargo.toml
[features]
full = ["mcp", "prompt-templates", "extensions", "builtin-tools"]
builtin-tools = ["tools-edit", "tools-search", "tools-web", "tools-collaboration"]
tools-edit = ["tools-edit-file", "tools-write-file", "tools-shell-exec", "tools-create-dir", "tools-copy-path", "tools-delete-path", "tools-move-path"]
tools-search = ["tools-read-file", "tools-grep", "tools-find-path", "tools-list-dir"]
mcp = ["dep:rmcp"]
extensions = ["dep:wasmtime", "dep:walkdir"]
prompt-templates = ["dep:minijinja"]
obscura = ["dep:obscura"]
tracing = ["dep:fastrace", "dep:fastrace-reqwest", "fastrace/enable", "elph-ai/tracing"]
```

## Navigation

Start here, then explore:

- [Architecture Overview](architecture/overview.md) — crate dependency graph, agent loop phases, session persistence
- [Source Map](architecture/source-map.md) — crate-by-crate module map with file paths
- [Agent Loop](workflows/agent-loop.md) — turn cycle: `AgentHarness::prompt()` → `run_agent_loop()` → tool execution → compaction
- [Compaction](workflows/compaction.md) — context window management, summarization, timestamp-gated estimate
- [Auth](workflows/auth.md) — CredentialStore, ModelsStore, OAuth providers, resolve_provider_auth
- [Providers](domains/providers.md) — ProviderStreams trait, 30+ provider adapters, compat flags
- [MCP](domains/mcp.md) — Model Context Protocol: transports, AES-256-GCM encryption, tool naming
- [Tools](domains/tools.md) — AgentTool, built-in tools, feature flags
- [Skills](domains/skills.md) — SKILL.md format, resolution, MiniJinja templates
- [Operations](operations.md) — CLI subcommands, ELPH_ env vars, observability
- [Testing](testing.md) — unit/integration test patterns, live provider tests
- [Pi Port Status](integrations/pi-port.md) — upstream commit cee5ff75, crate mapping, parity gaps

## Backlog

| Area                   | Source Anchor                                            | Reason Deferred                                                                  |
| ---------------------- | -------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Memory (floppy)        | `crates/floppy/src/`                                     | Vector memory, Welford scoring, embedding details — large specialized domain     |
| Extensions (WASM)      | `crates/elph-agent/src/plugins/`, `elph/src/extensions/` | Plugin system is evolving; `extensions/say-hello` is the only example            |
| Terminal UI (elph-tui) | `crates/elph-tui/src/`                                   | Component library with 15+ widgets — separate doc needed                         |
| Agent Modes            | `elph/src/types.rs`                                      | Build/Plan/Ask/Brave modes — documented in overview but needs mode-specific page |
| Prompts & Templates    | `elph/templates/agent/`, `crates/elph-agent/src/prompt/` | MiniJinja template engine, system prompt builder — separate domain page needed   |
| ACP Protocol           | `elph/src/cli/acp.rs`, `elph/src/platform/acp/`          | Agent Client Protocol server — needs its own integration page                    |
