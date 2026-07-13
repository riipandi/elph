# CLI

Design for the `elph` command-line interface.

## Invocation

```
elph [OPTIONS] [COMMAND]
```

## Global options

| Flag              | Description   |
| ----------------- | ------------- |
| `-V`, `--version` | Print version |
| `-h`, `--help`    | Print help    |

## Subcommands

| Command       | Description                                 |
| ------------- | ------------------------------------------- |
| `acp`         | Agent Client Protocol server over stdio     |
| `codegraph`   | Structural knowledge graph for code reviews |
| `completions` | Shell completion scripts                    |
| `doctor`      | Show discovered configuration               |
| `export`      | Export session transcript or archive        |
| `import`      | Import sessions                             |
| `mcp`         | MCP server configuration                    |
| `memory`      | Inspect and manage agent memory             |
| `models`      | List available models                       |
| `plugin`      | Plugins and extensions                      |
| `provider`    | AI providers and credentials                |
| `run`         | Non-interactive prompt → stdout             |
| `server`      | Local REST + WebSocket + web UI             |
| `session`     | List, search, restore sessions              |
| `stats`       | Token usage and cost                        |
| `update`      | Check for updates                           |
| `version`     | Print version                               |
| `worktree`    | Git worktrees                               |

### Default (no subcommand)

Launch the interactive TUI. Design: full agent experience; current gap documented in [openwiki](../openwiki/quickstart.md).

### `version`

Print version and exit. Equivalent to `-V`.

### `memory`

Inspect project-local memory at `<project>/.elph/store.db`.

| Subcommand       | Description                                             |
| ---------------- | ------------------------------------------------------- |
| `status`         | Counts, categories, top memories, task stats            |
| `list`           | All memories; optional `--category`                     |
| `tasks`          | Recent tasks (`--limit`, default 10)                    |
| `log`            | Event timeline (`--limit`, default 20)                  |
| `search <query>` | Semantic search (requires embedder)                     |
| `purge`          | Delete low-weight memories (`--threshold`, default 0.5) |

See [memory.md](./memory.md).

### `run`

| Flag               | Description                    |
| ------------------ | ------------------------------ |
| `-m`, `--model`    | Model (`provider/model`)       |
| `--output-format`  | Output format (default `text`) |
| `-c`, `--continue` | Continue recent session        |
| `-s`, `--session`  | Resume by session ID           |
| `--fork`           | Fork before continue           |
| `-f`, `--file`     | Attach files (repeatable)      |
| `-b`, `--brave`    | Auto-approve tools             |

### `provider`

Subcommands: `list`, `connect`, `disconnect`, `add`, `remove`, `catalog`, `update`.

Design: interactive credential setup, models.dev catalog sync, enable/disable providers.

### `codegraph`

Subcommands: `build`, `update`, `watch`, `status`, `changes`, `eval`, `postprocess`, `repos`, `register`, `unregister`, `visualize`, `serve`, `wiki`.

Structural graph for smarter code reviews and impact analysis.

### `mcp`

Subcommands: `list`, `add`, `remove`, `doctor`, `auth`, `logout`.

### `plugin`

Manage WASM extension bundles (wasmtime + Component Model). See [extensions.md](./extensions.md).

| Subcommand       | Flags             | Design behavior                                   |
| ---------------- | ----------------- | ------------------------------------------------- |
| `list`           | —                 | Installed extensions, enabled state, `/commands`  |
| `install <path>` | `--force`         | Copy local bundle to `~/.elph/extensions/<name>/` |
| `remove <name>`  | —                 | Delete global bundle                              |
| `enable <name>`  | —                 | Remove from `extensions.json` `disabled`          |
| `disable <name>` | —                 | Add to `extensions.json` `disabled`               |
| `update`         | `--all`, `[name]` | **Planned** — git/npm package updates             |

### `server`

Subcommands: `run`, `ps`, `kill`, `rotate-token`. Flags: `--port`, `--host`, `--foreground`.

### `session`

Subcommands: `list`, `search`, `delete`.

### `worktree`

Subcommands: `list`, `show`, `rm`, `gc`, `db`.

### `export` / `import`

Export formats: `json`, `markdown`, `zip`. Flags: `--output`, `--clipboard`, `--sanitize`.

## Bootstrap

First run scaffolds home config, data dirs, default settings, provider directory, project `.elph/` gitignore, and version metadata. Datastore (`metadata.db`) initializes for the default TUI and datastore-dependent subcommands.

## Exit codes

| Code  | Meaning                |
| ----- | ---------------------- |
| `0`   | Success                |
| `1`   | General error          |
| `3`   | Not authenticated      |
| `4`   | Permission denied      |
| `5`   | Rate limited           |
| `6`   | Network failure        |
| `7`   | API server error (5xx) |
| `130` | Interrupted (SIGINT)   |

## Workspace builds

Release builds via root `Makefile`. See [development.md](./development.md).

| Target            | Output                |
| ----------------- | --------------------- |
| `make build`      | `target/release/elph` |
| `make build-elph` | `target/release/elph` |

## Related

- [configuration.md](./configuration.md)
- [development.md](./development.md)
- [extensions.md](./extensions.md)
- [README.md](./README.md)
