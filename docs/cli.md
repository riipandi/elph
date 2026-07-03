# Elph CLI Documentation

## Usage

```
elph [OPTIONS] [COMMAND]
```

## Global Options

| Flag              | Description               |
|-------------------|---------------------------|
| `-V`, `--version` | Print version information |
| `-h`, `--help`    | Print help                |

## Subcommands

| Command       | Description                                                          |
|---------------|----------------------------------------------------------------------|
| `acp`         | Run Elph as an Agent Client Protocol (ACP) server over stdio         |
| `completions` | Generate shell completion scripts (bash, zsh, fish, powershell, etc) |
| `doctor`      | Show the configuration Elph discovers for this directory             |
| `export`      | Export a session transcript or archive                               |
| `import`      | Import sessions into Elph                                            |
| `mcp`         | Manage MCP server configurations                                     |
| `models`      | List available models and exit                                       |
| `plugin`      | Manage plugins and extensions                                        |
| `provider`    | Manage AI providers and credentials                                  |
| `run`         | Run a prompt non-interactively and exit                              |
| `server`      | Run the local Elph server (REST + WebSocket + web UI)                |
| `session`     | List, search, or restore sessions                                    |
| `stats`       | Show token usage and cost statistics                                 |
| `update`      | Check for updates or install a specific version                      |
| `version`     | Print version information                                            |
| `worktree`    | Manage git worktrees                                                 |

### Default (no subcommand)

When no subcommand is given, Elph launches the interactive TUI.

### `version`

Prints the Elph version and exits.

```
elph version
```

Equivalent to the `--version` global flag.

### `run`

Run a prompt non-interactively and print the response to stdout.

```
elph run [OPTIONS] [PROMPT]...
```

| Argument / Flag            | Description                                                               |
|----------------------------|---------------------------------------------------------------------------|
| `[PROMPT]...`              | Prompt to process non-interactively                                       |
| `-m`, `--model <MODEL>`    | Model to use (`provider/model`)                                           |
| `--output-format <FORMAT>` | Output format (default: `text`)                                           |
| `-c`, `--continue`         | Continue the most recent session for the current working directory        |
| `-s`, `--session <ID>`     | Resume a specific session by ID                                           |
| `--fork`                   | Fork the session before continuing (requires `--continue` or `--session`) |
| `-f`, `--file <FILE>`      | File(s) to attach to the prompt (repeatable)                              |
| `-y`, `--yolo`             | Auto-approve tool executions                                              |

### `completions`

Generate shell completion scripts.

```
elph completions [OPTIONS]
```

| Flag                    | Description                  |
|-------------------------|------------------------------|
| `-s`, `--shell <SHELL>` | Shell type (default: `bash`) |

### `doctor`

Show the configuration Elph discovers for the current directory.

```
elph doctor [OPTIONS]
```

| Flag     | Description                       |
|----------|-----------------------------------|
| `--json` | Emit machine-readable JSON output |

### `export`

Export a session transcript or archive.

```
elph export [OPTIONS] [SESSION_ID]
```

| Argument / Flag         | Description                                                   |
|-------------------------|---------------------------------------------------------------|
| `[SESSION_ID]`          | Session ID to export (exports most recent if omitted)         |
| `-o`, `--output <PATH>` | Output file path (default: stdout)                            |
| `--format <FORMAT>`     | Output format: `json`, `markdown`, or `zip` (default: `json`) |
| `-c`, `--clipboard`     | Copy to clipboard instead of writing to stdout                |
| `--sanitize`            | Redact sensitive transcript and file data                     |

### `import`

Import sessions into Elph.

```
elph import [OPTIONS] [FILE]
```

| Argument / Flag | Description                                   |
|-----------------|-----------------------------------------------|
| `[FILE]`        | Path to session file, directory, or share URL |
| `--list`        | List available sessions without importing     |
| `--json`        | Emit NDJSON output to stdout                  |

### `models`

List available models and exit.

```
elph models [OPTIONS] [PROVIDER]
```

| Argument / Flag    | Description                         |
|--------------------|-------------------------------------|
| `[PROVIDER]`       | Filter models by provider name      |
| `--search <QUERY>` | Fuzzy search filter for model names |

### `stats`

Show token usage and cost statistics.

```
elph stats [OPTIONS]
```

| Flag                     | Description                       |
|--------------------------|-----------------------------------|
| `--session <SESSION_ID>` | Filter statistics to one session  |
| `--json`                 | Emit machine-readable JSON output |

### `update`

Check for updates or install a specific version.

```
elph update [OPTIONS]
```

| Flag                  | Description                                                          |
|-----------------------|----------------------------------------------------------------------|
| `--check`             | Check for updates without installing                                 |
| `--json`              | Emit machine-readable JSON output (for `--check`)                    |
| `--force-reinstall`   | Force re-download and install even if already up to date             |
| `--version <VERSION>` | Install a specific version (e.g. `0.0.0` or `0.0.0-canary`)          |
| `--canary`            | Switch to the canary release channel (faster updates, may have bugs) |
| `--stable`            | Switch to the stable release channel (default, weekly releases)      |

### `provider`

Manage AI providers and credentials.

```
elph provider <COMMAND>
```

| Subcommand              | Description                                          |
|-------------------------|------------------------------------------------------|
| `list`                  | List configured providers and credentials            |
| `connect [provider]`    | Connect to an AI provider (interactive login)        |
| `disconnect [provider]` | Disconnect from a provider and clear credentials     |
| `add <url>`             | Import providers from a custom registry (`api.json`) |
| `remove <provider_id>`  | Remove a provider and its model aliases              |
| `catalog`               | Discover and import providers from models.dev        |
| `update [provider_id]`  | Update provider metadata and credentials             |

**`provider list` options**

| Flag     | Description                       |
|----------|-----------------------------------|
| `--json` | Emit machine-readable JSON output |

### `plugin`

Manage plugins and extensions.

```
elph plugin <COMMAND>
```

| Subcommand         | Description                                   |
|--------------------|-----------------------------------------------|
| `list`             | List installed plugins                        |
| `install <source>` | Install a plugin from a git URL or local path |
| `remove <name>`    | Remove an installed plugin                    |
| `update [name]`    | Update one or all installed plugins           |
| `enable <name>`    | Enable a disabled plugin                      |
| `disable <name>`   | Disable a plugin without uninstalling it      |

**`plugin install` options**

| Flag             | Description                                       |
|------------------|---------------------------------------------------|
| `--trust`        | Trust the plugin immediately (skip confirmation)  |
| `-g`, `--global` | Install in global config instead of project-local |
| `-f`, `--force`  | Replace existing plugin version                   |

**`plugin remove` options**

| Flag             | Description                                        |
|------------------|----------------------------------------------------|
| `-g`, `--global` | Remove from global config instead of project-local |

**`plugin update` options**

| Flag    | Description                  |
|---------|------------------------------|
| `--all` | Update all installed plugins |

### `mcp`

Manage MCP server configurations.

```
elph mcp <COMMAND>
```

| Subcommand            | Description                                        |
|-----------------------|----------------------------------------------------|
| `list`                | List configured MCP servers                        |
| `add <name> [config]` | Add or update an MCP server configuration          |
| `remove <name>`       | Remove an MCP server configuration                 |
| `doctor`              | Diagnose MCP server configuration and connectivity |
| `auth <name>`         | Authenticate with an OAuth-enabled MCP server      |
| `logout <name>`       | Remove OAuth credentials for an MCP server         |

### `session`

List, search, or restore sessions.

```
elph session <COMMAND>
```

| Subcommand       | Description                                           |
|------------------|-------------------------------------------------------|
| `list`           | List recent sessions (same as `search` with no query) |
| `search [query]` | Search sessions by keyword                            |
| `delete <id>`    | Permanently delete a session from history             |

### `server`

Run the local Elph server (REST + WebSocket + web UI).

```
elph server [OPTIONS] [COMMAND]
```

| Flag                  | Description                                |
|-----------------------|--------------------------------------------|
| `-p`, `--port <PORT>` | Port to listen on (default: `8080`)        |
| `--host <HOST>`       | Hostname to bind to (default: `127.0.0.1`) |

| Subcommand     | Description                                          |
|----------------|------------------------------------------------------|
| `run`          | Start the Elph server (background daemon by default) |
| `ps`           | List clients connected to the running server         |
| `kill`         | Stop the running server                              |
| `rotate-token` | Generate a new persistent server token               |

**`server run` options**

| Flag           | Description                                  |
|----------------|----------------------------------------------|
| `--foreground` | Run in the foreground instead of as a daemon |

### `worktree`

Manage git worktrees.

```
elph worktree <COMMAND>
```

| Subcommand          | Description                              |
|---------------------|------------------------------------------|
| `list`              | List tracked worktrees                   |
| `show <id_or_path>` | Show details for a specific worktree     |
| `rm <id_or_path>`   | Remove worktrees                         |
| `gc`                | Garbage-collect orphaned/stale worktrees |
| `db`                | Database maintenance                     |

**`worktree rm` options**

| Flag            | Description                 |
|-----------------|-----------------------------|
| `-f`, `--force` | Remove without confirmation |

### `help`

Print the help message of a subcommand.

```
elph help [SUBCOMMAND]
```

## Implementation status

All subcommands exist as placeholders — they print a "not yet implemented" message except for `version` and the default TUI.

## Exit Codes

| Code  | Meaning                      | Constant                 |
|-------|------------------------------|--------------------------|
| `0`   | Success                      | `EXIT_SUCCESS`           |
| `1`   | General error                | `EXIT_ERROR`             |
| `3`   | Not authenticated            | `EXIT_AUTH_ERROR`        |
| `4`   | Permission denied            | `EXIT_PERMISSION_DENIED` |
| `5`   | Rate limit exceeded          | `EXIT_RATE_LIMITED`      |
| `6`   | Network failure              | `EXIT_CONNECTION_ERROR`  |
| `7`   | API server error (5xx)       | `EXIT_SERVER_ERROR`      |
| `130` | Interrupted (SIGINT/SIGTERM) | `EXIT_INTERRUPTED`       |
