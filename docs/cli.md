# Elph CLI Documentation

## Usage

```
elph [COMMAND]
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
| `export`      | Export a session as a ZIP archive                                    |
| `import`      | Import sessions into Elph                                            |
| `mcp`         | Manage MCP server configurations                                     |
| `models`      | List available models and exit                                       |
| `provider`    | Manage AI providers and credentials                                  |
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
| `-h`, `--help`        | Print help                                                           |

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
