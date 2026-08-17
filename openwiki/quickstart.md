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
Cargo.toml          # workspace root — resolver = "2", members = ["crates/coding-agent", "crates/elph-agent", "crates/elph-ai", "crates/floppy"]
Makefile            # build, test, lint, release, cross-compilation targets
crates/
├── coding-agent/   # product binary + library (CLI, TUI, agent session orchestration) — was elph/
├── elph-agent/     # app-agnostic agent runtime (port of @earendil-works/pi-agent)
├── elph-ai/        # unified LLM API with provider collections, auth, streaming (port of @earendil-works/pi-ai)
├── elph-tui/       # reusable iocraft-based TUI components
├── floppy/         # AI memory with vector search (Turso); port of memelord; metal feature for Apple Silicon
├── control-plane/  # excluded from workspace
├── elph-cron/      # skeleton — excluded from workspace
├── elph-sandbox/   # skeleton — excluded from workspace
├── elph-swarm/     # skeleton — excluded from workspace
├── ext-hello/      # WASM extension example — excluded from workspace
├── rendown/        # markdown renderer with streaming support — excluded from workspace
└── vendor/iocraft/ # patched iocraft (bracketed-paste support)
docs/               # project documentation, porting status, design notes
skills/             # OpenWiki skill files (mermaid-diagrams, migrate-wiki, write-connector)
extensions/         # WASM extension plugins (say-hello)
templates/agent/    # MiniJinja system prompt templates (coding_base.txt, mode_*.md, session_title_*.md)
```

> **Note:** `elph-db` was removed in commit `eba87a7`. Its open/connect/retry/lock-error helpers were absorbed into `crates/elph-agent/src/datastore/conn.rs`.

## Make Targets

| Target                 | Description                                                                                                    |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| `make build`           | Build `elph` binary (debug; `RELEASE=1` or `-- --release` for release; `PROFILE=dist` or `-- --dist` for dist) |
| `make run`             | Run elph coding agent                                                                                          |
| `make watch`           | Run with hot reload (requires watchexec)                                                                       |
| `make test`            | Run all workspace tests via `cargo nextest`                                                                    |
| `make test-elph`       | Test `elph-ai` + `elph` + `elph-agent` (with `--features full`)                                                |
| `make test-elph-tui`   | Test `elph-tui`                                                                                                |
| `make check`           | Check compilation (no codegen)                                                                                 |
| `make check-elph`      | Check `elph-ai` + `elph` + `elph-agent` compile (with `--features full`)                                       |
| `make check-elph-tui`  | Check `elph-tui` compiles (lib, tests, examples)                                                               |
| `make lint`            | Run clippy with `-D warnings` (`elph`, `elph-agent`, `elph-ai`)                                                |
| `make fmt`             | Format all Rust code + models + wiki                                                                           |
| `make coverage`        | Test coverage via `cargo-llvm-cov`                                                                             |
| `make generate-models` | Regenerate `elph-ai` model catalogs from pi upstream                                                           |
| `make cross`           | Cross-compile one platform (`CROSS_TARGET=<triple>`)                                                           |
| `make cross-pull`      | Pull cross-compilation Docker images                                                                           |
| `make release`         | Host-aware release build                                                                                       |
| `make install`         | Install to `~/.local/bin/` (debug → `elph-debug`, release → `elph-canary`, dist → `elph`)                      |
| `make prepare`         | Prepare workspace for development                                                                              |
| `make clean`           | Clean build artifacts                                                                                          |
| `make stats`           | Show sccache stats and line counts                                                                             |
| `make publish`         | Publish crates                                                                                                 |
| `make version`         | Show version info                                                                                              |

**Feature flags:** `make build -- --features metal` enables macOS GPU acceleration for local embeddings (auto-detected on Apple Silicon). The `metal` feature is forwarded to `floppy/metal` via `crates/coding-agent/Cargo.toml` (commit `3f15161`).

## Crate Dependency Graph

```
elph (binary + lib)
├── elph-agent (--features "tracing, builtin-tools, mcp, prompt-templates, extensions")
│   ├── elph-ai (--features "tracing")
│   └── floppy (memory, --features "full")
├── elph-ai
├── elph-tui (iocraft-based TUI components)
├── floppy
└── git2, iocraft, tokio, clap, turso, etc.
```

> **Note:** `elph-db` was removed in commit `eba87a7`. Its helpers were absorbed into `elph-agent/src/datastore/conn.rs`.

The `elph` crate (in `crates/coding-agent/`) is the product binary. Its `lib.rs` exports modules for:

- `cli/` — 18 subcommands (ACP, codegraph, provider, run, server, session, etc.)
- `agent/` — session orchestration above `elph-agent` (modes, prompts, tools, MCP bootstrap)
- `tui/` — interactive TUI shell powered by `iocraft`
- `platform/` — paths, settings, datastore, bootstrap, ACP, migrations
- `memory/` — floppy memory hooks, commands, tools
- `codegraph/` — semantic code index and impact graph
- `extensions/` — WASM extension host
- `command/` — shell command helpers
- `types/` — AgentMode, ThinkingLevel, ScopedModel, AgentModeKind
- `utils/` — shared utilities
- `worktree/` — git worktree management

## Key Feature Flags in `elph-agent`

```toml
# crates/elph-agent/Cargo.toml
[features]
default = []
full = ["mcp", "prompt-templates", "extensions", "builtin-tools"]
builtin-tools = ["tools-edit", "tools-search", "tools-web", "tools-collaboration"]
tools-edit = ["tools-edit-file", "tools-write-file", "tools-shell-exec", "tools-create-dir", "tools-copy-path", "tools-delete-path", "tools-move-path"]
tools-search = ["tools-read-file", "tools-grep", "tools-find-path", "tools-list-dir"]
mcp = ["dep:rmcp"]
extensions = ["dep:wasmtime", "dep:walkdir"]
prompt-templates = []
tracing = ["dep:fastrace", "dep:fastrace-reqwest", "fastrace/enable", "elph-ai/tracing"]
```

## Navigation

Start here, then explore:

- [Architecture Overview](architecture/overview.md) — crate dependency graph, agent loop phases, session persistence, worker runtime
- [Source Map](architecture/source-map.md) — crate-by-crate module map with file paths
- [Agent Loop](workflows/agent-loop.md) — turn cycle: `AgentHarness::prompt()` → `run_agent_loop()` → tool execution → compaction
- [Workers](workflows/workers.md) — multi-process worker coordination, session leases, file leases, mailbox
- [Handover](workflows/handover.md) — foreign session import (Claude, Codex) with inert safety boundary
- [Compaction](workflows/compaction.md) — context window management, summarization, timestamp-gated estimate
- [Auth](workflows/auth.md) — CredentialStore, ModelsStore, OAuth providers, resolve_provider_auth
- [Providers](domains/providers.md) — ProviderStreams trait, 30+ provider adapters, compat flags
- [MCP](domains/mcp.md) — Model Context Protocol: transports, AES-256-GCM encryption, tool naming
- [Tools](domains/tools.md) — AgentTool, built-in tools, shell_use, list_skills, feature flags
- [Skills](domains/skills.md) — SKILL.md format, resolution, MiniJinja templates, list_skills tool
- [Subagents](domains/subagents.md) — subagent output durability, TurnGuard, wait-for-output
- [Operations](operations.md) — CLI subcommands, headless mode, ELPH_ env vars, slash commands, observability
- [Testing](testing.md) — unit/integration test patterns, live provider tests
- [Pi Port Status](integrations/pi-port.md) — upstream commit cee5ff75, crate mapping, parity gaps, elph-only extensions

## Change Navigation (Task Routing)

| Change Area / Intent                 | Relevant Page(s)                                                                | Source Entry Points & Symbols                                                                                    | Focused Tests                                                                             | Minimal Validation Command                                   |
| ------------------------------------ | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Add/modify agent loop turn behavior  | [Agent Loop](workflows/agent-loop.md), [Architecture](architecture/overview.md) | `crates/elph-agent/src/runtime/run_loop.rs`, `prompt_ops.rs`, `loop_config.rs`, `AgentLoopConfig`                | `crates/elph-agent/tests/agent_loop.rs`, `harness.rs`                                     | `cargo test -p elph-agent --test agent_loop`                 |
| Add/modify tool (built-in)           | [Tools](domains/tools.md)                                                       | `crates/elph-agent/src/tools/`, `mod.rs` (feature-gated registration), `types.rs` (`AgentTool`, `ToolExecuteFn`) | Inline tests in tool file, `crates/elph-agent/tests/harness.rs`                           | `cargo test -p elph-agent --lib tools`                       |
| Add/modify MCP transport or registry | [MCP](domains/mcp.md)                                                           | `crates/elph-agent/src/tools/mcp/`, `registry/mod.rs`, `client.rs`, `config.rs`, `session.rs`                    | `crates/elph-agent/tests/mcp_deepwiki.rs`                                                 | `cargo test -p elph-agent --test mcp_deepwiki`               |
| Add/modify LLM provider adapter      | [Providers](domains/providers.md)                                               | `crates/elph-ai/src/providers/builtin.rs`, `adapter.rs`, `crates/elph-ai/src/types/mod.rs` (`CompatFlags`)       | `crates/elph-ai/tests/e2e_live.rs`, `openai_completions_compat_gaps.rs`                   | `cargo test -p elph-ai --lib providers`                      |
| Add/modify OAuth provider            | [Auth](workflows/auth.md)                                                       | `crates/elph-ai/src/auth/oauth/`, `helpers.rs`, `resolve.rs`, `credential_store.rs`                              | `crates/elph-ai/tests/oauth_auth.rs`                                                      | `cargo test -p elph-ai --test oauth_auth`                    |
| Change session persistence or GC     | [Architecture](architecture/overview.md)                                        | `crates/elph-agent/src/session/`, `retention.rs`, `crates/elph-agent/src/turns/store.rs`, `todos/store.rs`       | `crates/elph-agent/tests/session.rs`, `turso_session.rs`, `storage.rs`                    | `cargo test -p elph-agent --test session`                    |
| Modify compaction strategy           | [Compaction](workflows/compaction.md)                                           | `crates/elph-agent/src/compaction/`, `estimation.rs`, `compact.rs`, `summarization.rs`, `compaction_ops.rs`      | `crates/elph-agent/tests/compaction.rs`                                                   | `cargo test -p elph-agent --test compaction`                 |
| Add/modify subagent feature          | [Subagents](domains/subagents.md)                                               | `crates/elph-agent/src/agent/subagent/`, `types.rs`, `control.rs`, `harness.rs`                                  | `crates/elph-agent/tests/subagent.rs`                                                     | `cargo test -p elph-agent --test subagent`                   |
| Modify multi-worker coordination     | [Workers](workflows/workers.md)                                                 | `crates/elph-agent/src/workers/`, `crates/coding-agent/src/agent/worker_runtime.rs`                              | Inline tests in `lease.rs`, `file_lease.rs`, `crates/coding-agent/tests/workers_multi.rs` | `cargo test -p elph-agent --lib workers`                     |
| Modify foreign session handover      | [Handover](workflows/handover.md)                                               | `crates/coding-agent/src/agent/handover/`, `mod.rs`, `codex.rs`                                                  | `crates/coding-agent/src/agent/handover/tests.rs`, `codex/tests.rs`                       | `cargo test -p coding-agent -- agent::handover`              |
| Modify headless run mode             | [Operations](operations.md)                                                     | `crates/coding-agent/src/agent/run_mode.rs`, `headless_status.rs`, `pretty_markdown.rs`                          | Inline `#[cfg(test)]` in `run_mode.rs`                                                    | `cargo test -p coding-agent --lib agent::run_mode`           |
| Modify skill system                  | [Skills](domains/skills.md)                                                     | `crates/elph-agent/src/skills/`, `load/`, `format.rs`, `args.rs`, `crates/elph-agent/src/tools/list_skills.rs`   | `crates/elph-agent/tests/harness.rs` (skill invocation)                                   | `cargo test -p elph-agent --lib skills`                      |
| Change CLI or config behavior        | [Operations](operations.md)                                                     | `crates/coding-agent/src/cli/`, `platform/settings.rs`, `platform/paths.rs`                                      | `crates/coding-agent/tests/cli.rs`, `bootstrap.rs`                                        | `cargo test -p coding-agent --test cli`                      |
| Add provider model catalog           | [Providers](domains/providers.md)                                               | `crates/elph-ai/models/`, `crates/elph-ai/src/models/catalog.rs`, `bin/generate_models/`                         | Generated model tests                                                                     | `make generate-models && cargo test -p elph-ai --lib models` |
| Modify pi port parity                | [Pi Port](integrations/pi-port.md)                                              | `docs/porting/`, `crates/elph-ai/src/`, `crates/elph-agent/src/`                                                 | Targeted integration tests for affected crate                                             | `cargo test -p elph-agent -p elph-ai`                        |

## Backlog

| Area                   | Source Anchor                                                                 | Reason Deferred                                                                  |
| ---------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Memory (floppy)        | `crates/floppy/src/`                                                          | Vector memory, Welford scoring, embedding details — large specialized domain     |
| Extensions (WASM)      | `crates/elph-agent/src/plugins/`, `crates/coding-agent/src/extensions/`       | Plugin system is evolving; `extensions/say-hello` is the only example            |
| Terminal UI (elph-tui) | `crates/elph-tui/src/`                                                        | Component library with 15+ widgets — separate doc needed                         |
| Agent Modes            | `crates/coding-agent/src/types.rs`                                            | Build/Plan/Ask/Brave modes — documented in overview but needs mode-specific page |
| Prompts & Templates    | `crates/coding-agent/templates/agent/`, `crates/elph-agent/src/prompt/`       | MiniJinja template engine, system prompt builder — separate domain page needed   |
| ACP Protocol           | `crates/coding-agent/src/cli/acp.rs`, `crates/coding-agent/src/platform/acp/` | Agent Client Protocol server — needs its own integration page                    |
