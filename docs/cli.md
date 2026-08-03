# CLI

Design for the `elph` command-line interface.

## Invocation

```
elph [OPTIONS] [COMMAND]
```

## Global options

| Flag                         | Description                                                                 |
| ---------------------------- | --------------------------------------------------------------------------- |
| `-V`, `--version`            | Print version                                                               |
| `-h`, `--help`               | Print help                                                                  |
| `-c`, `--continue`           | Continue the **most recent session for this project** (CWD / `PROJECT_DIR`); does **not** start a new session |
| `-r`, `--resume <SESSION_ID>` | Resume a **specific** session by ID                                         |

### Default (no subcommand)

```sh
elph                      # new interactive session for this project
elph --continue           # reopen last session for this project
elph -c                   # same as --continue
elph --resume <id>        # reopen a specific session
elph -r <id>              # same as --resume
```

`--continue` and `--resume` are mutually exclusive. If this project has no prior sessions, `--continue` exits with an error (it will not invent a new session).

## Subcommands

| Command       | Description                                 |
| ------------- | ------------------------------------------- |
| `acp`         | Agent Client Protocol server over stdio     |
| `codegraph`   | Semantic code index + shallow impact graph  |
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

Launch without a subcommand starts the interactive TUI (see global `--continue` / `--resume` above).

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

| Flag                   | Description                       |
| ---------------------- | --------------------------------- |
| `-m`, `--model`        | Model (`provider/model`)          |
| `--output-format`      | Output format (default `text`)    |
| `-c`, `--continue`     | Continue last session for this project |
| `-r`, `--resume`       | Resume by session ID (alias: `--session`) |
| `--fork`               | Fork before continue              |
| `-f`, `--file`         | Attach files (repeatable)         |
| `-b`, `--brave`        | Auto-approve tools                |
| `--max-retries`        | Max retry attempts (default 3)    |
| `--max-backoff-ms`     | Max backoff delay in ms           |
| `--circuit-threshold`  | Circuit breaker failure threshold |
| `--circuit-timeout-ms` | Circuit breaker recovery timeout  |

### `provider`

Subcommands: `list`, `connect`, `disconnect`, `add`, `remove`, `catalog`, `update`.

Design: interactive credential setup, models.dev catalog sync, enable/disable providers.

### `codegraph`

Subcommands: `build`, `update`, `status`, `search`, `impact`, `purge`.

Semantic code index (hybrid FTS + vector) and shallow impact graph in project `store.db`. See [codegraph.md](./codegraph.md).

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

First run scaffolds home config, data dirs, default settings, project `.elph/` gitignore, version metadata, global `AGENTS.md`, and **unpacks** built-in provider catalogs into `CONFIG_DIR/providers/*.json` (only missing files). Datastore (`metadata.db`) initializes for the default TUI and datastore-dependent subcommands.

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
