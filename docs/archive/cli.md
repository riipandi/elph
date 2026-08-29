# CLI

Design for the `elph` command-line interface.

## Invocation

```
elph [OPTIONS] [COMMAND]
```

## Global options

| Flag                          | Description                                                                                                   |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `-V`, `--version`             | Print version                                                                                                 |
| `-h`, `--help`                | Print help                                                                                                    |
| `-c`, `--continue`            | Continue the **most recent session for this project** (CWD / `PROJECT_DIR`); does **not** start a new session |
| `-r`, `--resume <SESSION_ID>` | Resume a **specific** session by ID                                                                           |

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

| Command       | Description                                |
| ------------- | ------------------------------------------ |
| `acp`         | Agent Client Protocol server over stdio    |
| `completions` | Shell completion scripts                   |
| `doctor`      | Show discovered configuration              |
| `export`      | Export session transcript or archive       |
| `import`      | Import sessions                            |
| `mcp`         | MCP server configuration                   |
| `memory`      | Inspect and manage agent memory            |
| `models`      | List available models                      |
| `plugin`      | Plugins and extensions                     |
| `provider`    | AI providers and credentials               |
| `run`         | Non-interactive prompt → stdout            |
| `server`      | Local REST + WebSocket + web UI            |
| `session`     | List, search, restore sessions             |
| `stats`       | Token usage and cost                       |
| `update`      | Check for updates                          |
| `version`     | Print version                              |
| `worktree`    | Git worktrees                              |

Launch without a subcommand starts the interactive TUI (see global `--continue` / `--resume` above).

### `version`

Print version and exit. Equivalent to `-V`.

### `update`

Update the installed Elph binary from GitHub Releases. Stable releases are
selected by default; use `--canary` for the canary channel or `--stable` to
select stable explicitly. `--check` checks without installing and
`--check --json` emits a machine-readable status. `--version <VERSION>` pins a
release, and `--force-reinstall` reinstalls the selected release even when it
matches the installed version.

Human-readable output uses a compact status line; an installation also reports
the version transition and the platform archive being downloaded. `--json`
remains machine-readable and is only available with `--check`.

The archive is verified against its matching entry in `SHA256SUMS` before the
binary is replaced. The installed version and channel check timestamps are
recorded in `APP_DATA/version.json`.

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

| Flag                   | Description                               |
| ---------------------- | ----------------------------------------- |
| `-m`, `--model`        | Model (`provider/model`)                  |
| `--output-format`      | Output format (default `text`)            |
| `-c`, `--continue`     | Continue last session for this project    |
| `-r`, `--resume`       | Resume by session ID (alias: `--session`) |
| `--fork`               | Fork before continue                      |
| `-f`, `--file`         | Attach files (repeatable)                 |
| `-b`, `--brave`        | Auto-approve tools                        |
| `--max-retries`        | Max retry attempts (default 3)            |
| `--max-backoff-ms`     | Max backoff delay in ms                   |
| `--circuit-threshold`  | Circuit breaker failure threshold         |
| `--circuit-timeout-ms` | Circuit breaker recovery timeout          |

### `models`

List models from the embedded catalogs baked into the binary (source: `elph-ai` built-ins).

| Argument / flag    | Description                                                                           |
| ------------------ | ------------------------------------------------------------------------------------- |
| `[PROVIDER]`       | Optional positional provider filter (matches `id` or display name, case-insensitive). |
| `--search <QUERY>` | Fuzzy filter across `provider.id`, `model.id`, and `model.name`.                      |

Output is grouped by provider. A summary line prints the provider/model counts (and the active query when `--search` is used), followed by one section per provider. Each model line shows the display name, the model id (dimmed), and a compact spec: context window (e.g. `200k`, `1.0M`) plus per-million-token price (e.g. `$5.00/$25.00`; `free` when both rates are zero), tagged `reasoning` when the model supports thinking.

```text
Models
──────
  Providers  1
  Models     15

Anthropic (anthropic)
─────────────────────
  Claude Opus 4.5 (latest)   claude-opus-4-5   · 200k ctx · $5.00/$25.00 per M · reasoning
```

When nothing matches, a `No models matched.` line is printed with the applied filters shown.

### `provider`

Subcommands: `list`, `connect`, `disconnect`, `update`.

- `list` — list configured providers and stored credentials.
- `connect [id]` — sign in to a provider (interactive OAuth/API key, or `--env VAR` to read a key from an env var).
- `disconnect [id]` — sign out and clear stored credentials.
- `update [id]` — refresh model catalogs from the embedded seed baked into the binary.

`elph provider update` writes catalogs to `~/.config/elph/providers/<provider>.json`. It compares the
embedded seed against each on-disk file and reconciles conflicts:

- **New** — no file yet; the seed is written.
- **Up to date** — file already matches the seed; nothing happens.
- **Conflict** — the file differs from the seed. By default it **merges**: your on-disk file is kept and
  only seed models that are missing are added, so your custom configuration is never overwritten.

Flags:

| Flag          | Description                                                                |
| ------------- | -------------------------------------------------------------------------- |
| `--yes`       | Apply to all providers without prompting (merge; keeps custom config).     |
| `--overwrite` | Replace existing catalogs with the embedded seed (discards custom config). |
| `--dry-run`   | Show what would change without writing anything.                           |

When conflicts exist and neither `--yes` nor `--overwrite` is given, the CLI opens an interactive
`inquire` selector for each conflicting provider (arrow keys, Enter to choose, Esc to quit):

- **Update (keep custom config)** — merge: keep your file, add missing seed models.
- **Skip this provider** — leave the existing file untouched.
- **Overwrite with embedded seed** — replace the file (discards custom config).
- **Show diff** — print a concise, field-level diff (added models and the exact fields that differ, e.g. `name` / `context_window` / `cost.input`), then re-prompt. No raw JSON is dumped.
- **Update all remaining** / **Skip all remaining** / **Overwrite all remaining** — apply the choice to every remaining conflict.
- **Quit** — abort with no changes.

Interactive prompts require a TTY; pipe a non-interactive run with conflicts through `--yes` or
`--overwrite`, otherwise the command errors.

### `mcp`

Subcommands: `list`, `add`, `remove`, `doctor`, `auth`, `logout`.

### `plugin`

Manage WASM extension bundles (wasmi). This historical command surface was removed; see the [current native hook design](../hooks.md).

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


Startup and scan phases (bootstrap, datastore init) render an interactive progress line on **stderr**: an animated spinner, a stepped bar with `pos/total`, the message, and a running elapsed timer (e.g. `⠙ Initializing databases [━╸──────────] 1/2 · 3s`). Pressing `Ctrl+C` during one of these phases aborts cleanly with `Interrupted.` and exit code `130`.

Diagnostic logs also go to **stderr**, keeping stdout reserved for program output (tables, stats, scan results). Piped stdout therefore stays machine-readable and free of log interleaving.

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
- [hooks.md](../hooks.md)
- [README.md](./README.md)
