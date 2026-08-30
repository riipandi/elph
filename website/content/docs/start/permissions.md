# Permissions

## Workspace trust

Trusted workspace directories are recorded in `CONFIG_DIR/trust.json`. On the
first interactive TUI launch in an untrusted directory, Elph asks whether to
trust the project before opening the datastore or TUI. Selecting Yes records
the canonical project path; selecting No exits. You can also use `/trust` in
the TUI to mark the current workspace. `/untrust` writes an explicit `false`
decision, but only works while the project is trusted; that local decision
also overrides trust inherited from a parent directory. The command palette
shows only the action that matches the current trust state. Project-local hooks
under `.elph/hooks.json` load only after trust.

## Tool policy

Built-in tools (edit, shell, web, …) and MCP tools follow host approval / mode settings. In Plan mode mutating workspace tools need one-shot approval (TUI/ACP); mutating MCP and multi-agent tools stay blocked. Headless Plan denies mutating tools. Allow once does not start Build; many approved writes still mutate the tree. Switching to implementation still requires the confirmation card.

## Logs

| Path                        | Content             |
| --------------------------- | ------------------- |
| `APP_DATA/logs/elph.jsonl`  | App log             |
| `APP_DATA/logs/crash-YYMMDDhh.jsonl` | Panic reports (UTC hour) |
| `APP_DATA/logs/mcp/`        | MCP stderr captures |

Skills and prompt templates are instructions for the agent — review third-party skills before enabling them. Bootstrap never overwrites existing user skill files under `CONFIG_DIR/skills/`.
