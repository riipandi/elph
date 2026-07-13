# Operations & CLI

This page covers the Elph CLI surface, configuration, paths, CI/CD, and publishing workflow.

## CLI Overview

**Source**: `/elph/src/cli/mod.rs`

The `elph` binary accepts:

```
elph [--resume <SESSION_ID>] [--version] [<COMMAND>]
```

Without a subcommand, elph starts the interactive TUI for the current directory.

### Subcommands

| Command       | Source                         | Purpose                                     |
| ------------- | ------------------------------ | ------------------------------------------- |
| `acp`         | `/elph/src/cli/acp.rs`         | Run as Agent Client Protocol server         |
| `codegraph`   | `/elph/src/cli/codegraph.rs`   | Structural knowledge graph for code reviews |
| `completions` | `/elph/src/cli/completions.rs` | Generate shell completions                  |
| `doctor`      | `/elph/src/cli/doctor.rs`      | Show resolved configuration                 |
| `export`      | `/elph/src/cli/export.rs`      | Export session transcripts                  |
| `import`      | `/elph/src/cli/import.rs`      | Import sessions                             |
| `mcp`         | `/elph/src/cli/mcp.rs`         | Manage MCP server configurations            |
| `memory`      | `/elph/src/cli/memory.rs`      | Inspect and manage agent memory (floppy)    |
| `models`      | `/elph/src/cli/models.rs`      | List available models                       |
| `provider`    | `/elph/src/cli/provider.rs`    | Manage provider configurations              |
| `run`         | `/elph/src/cli/run.rs`         | Non-interactive prompt execution            |
| `server`      | `/elph/src/cli/server.rs`      | Start ACP HTTP server                       |
| `session`     | `/elph/src/cli/session.rs`     | Manage sessions                             |
| `stats`       | `/elph/src/cli/stats.rs`       | Session statistics                          |
| `update`      | `/elph/src/cli/update.rs`      | Self-update                                 |
| `worktree`    | `/elph/src/cli/worktree.rs`    | Manage worktrees                            |

### Non-interactive Run

**Source**: `/elph/src/cli/run.rs`

```
elph run [OPTIONS] <PROMPT>
```

Options:

- `-m, --model <MODEL>` — Model to use (provider/model format)
- `--output-format <FORMAT>` — Output format (default: `text`)
- `-c, --continue` — Continue most recent session
- `-s, --session <SESSION_ID>` — Resume a specific session
- `--fork` — Fork session before continuing
- `-f, --file <FILE>` — File(s) to attach (not yet implemented)
- `-b, --brave` — Auto-approve tool executions

## Configuration

### Settings file

**Source**: `/elph/src/platform/settings.rs`

Stored as JSON in `~/.elph/settings.json`:

| Field                      | Type    | Default              | Description                  |
| -------------------------- | ------- | -------------------- | ---------------------------- |
| `syncInterval`             | string  | `"5s"`               | UI sync interval             |
| `theme`                    | string  | `"catppuccin"`       | Color theme                  |
| `showThinking`             | bool    | `true`               | Show thinking blocks         |
| `autoExpandThinking`       | bool    | `false`              | Auto-expand thinking         |
| `useRawPaste`              | bool    | `false`              | Raw paste mode               |
| `stickyScroll`             | bool    | `true`               | Sticky scroll in transcript  |
| `preferedResponseLanguage` | string  | `"English"`          | Response language preference |
| `session.providerId`       | string? | null                 | Default provider             |
| `session.modelId`          | string? | null                 | Default model                |
| `session.agentMode`        | string  | `"plan"`             | Agent mode (plan/default)    |
| `session.thinkingLevel`    | string  | `"medium"`           | Thinking budget level        |
| `database.url`             | string? | null                 | Turso database URL           |
| `database.token`           | string? | null                 | Turso database token         |
| `memory.embedModel`        | string  | `"all-MiniLM-L6-v2"` | Embedding model              |
| `memory.embedQuantized`    | bool    | `true`               | Quantized embeddings         |
| `autoCompactContext`       | bool    | `true`               | Auto-compact context         |
| `autoCompactLimit`         | u8      | `75`                 | Compact at % usage           |
| `footerTokenDisplay`       | string  | `"auto"`             | Token display mode           |

### MCP Configuration

**Source**: `/elph/src/platform/mcp.rs`

MCP servers configured in `~/.elph/mcp.json`. See [mcp-integration.md](mcp-integration.md) for format details.

### Paths

**Source**: `/elph/src/platform/paths.rs`

Elph uses a configurable path resolver:

| Directory | Default                | Purpose                          |
| --------- | ---------------------- | -------------------------------- |
| Config    | `~/.elph/`             | Settings, MCP config, extensions |
| Data      | `~/.local/share/elph/` | Sessions, logs                   |
| Project   | `pwd`                  | Project-local `.elph/` directory |

Env var overrides: `ELPH_HOME`, `ELPH_DATA_DIR`, `ELPH_PROJECT_DIR`

### Database

**Source**: `/elph/src/platform/datastore.rs`

Optional Turso/libSQL database for session storage and memory (floppy). Configured via `settings.json` or environment.

## Build System

**Source**: `/Makefile`

### Make targets

| Target                                                     | Description                            |
| ---------------------------------------------------------- | -------------------------------------- |
| `make build`                                               | Release build `elph` binary            |
| `make check`                                               | `cargo check --workspace`              |
| `make test`                                                | `cargo nextest run`                    |
| `make lint`                                                | `cargo clippy --workspace -D warnings` |
| `make fmt`                                                 | `cargo fmt`                            |
| `make run`                                                 | Run `elph` via cargo                   |
| `make watch`                                               | Run with hot reload (watchexec)        |
| `make install`                                             | Copy `elph-next` to `~/.local/bin`     |
| `make clean`                                               | Clean build artifacts                  |
| `make prepare`                                             | Install toolchain, setup hooks         |
| `make stats`                                               | Show crate sizes                       |
| `make publish`                                             | Publish all crates                     |
| `make cross`                                               | Cross-compile for other targets        |
| `make release-linux` / `release-macos` / `release-windows` | Build release binaries                 |

### Cargo workspace

**Source**: `/Cargo.toml`

Workspace members: `crates/elph-*`, `elph`
Excludes: `extensions/say-hello`

Key dependencies:

- `rmcp` — MCP client (gated behind `mcp` feature)
- `wasmtime` — WASM runtime (gated behind `extensions` feature)
- `fastembed` — Embedding models for memory
- `tuie` — TUI framework
- `toon-format` — Prompt encoding

## CI/CD

**Source**: `/.github/workflows/`

| Workflow        | File                  | Purpose                     |
| --------------- | --------------------- | --------------------------- |
| CI (app)        | `_ci-app.yml`         | Shared CI workflow for elph |
| Release (app)   | `_release-app.yml`    | Shared release workflow     |
| Version gate    | `_version-gate.yml`   | Version consistency checks  |
| OpenWiki update | `openwiki-update.yml` | Scheduled wiki refresh      |

### CI helpers

Scripts in `/scripts/`:

- `ci-check-app-version.sh` — Check version consistency
- `ci-check-release-version.sh` — Check release version
- `ci-set-app-version.sh` — Set application version
- `ci-sync-app-version-on-main.sh` — Sync version on main
- `cross-build.sh` / `cross-release.sh` — Cross-compilation helpers
- `publish-crates.sh` — Publish all crates to crates.io
- `version.sh` — Version management

## Publishing

**Source**: `/scripts/publish-crates.sh`

Publishing order (workspace crates):

1. `elph-core`
2. `elph-ai`
3. `elph-agent`
4. `elph-tui`
5. `elph` (binary — published for library users)

`make publish-dry-run` — Dry run publish to verify.

## Settings file location

Primary: `~/.elph/settings.json`
MCP config: `~/.elph/mcp.json`

Settings are loaded via `Settings::load(&paths)` in `/elph/src/platform/settings.rs`.

## Change guidance

- **New subcommand**: Add to `elph/src/cli/` module, register in `elph/src/cli/mod.rs`, add to `Commands` enum
- **New config field**: Add to `Settings` struct in `/elph/src/platform/settings.rs`
- **CI changes**: Modify `.github/workflows/_ci-app.yml` for app CI, `_release-app.yml` for releases
- **Publishing**: Test with `make publish-dry-run` before actual publish
- **Cross-compilation**: Modify `scripts/cross-build.sh` and `scripts/cross-release.sh`
- **Version bump**: Use `make bump` or modify `elph/Cargo.toml` version field
- **Settings migration**: Check `Settings::load` for backward compatibility
