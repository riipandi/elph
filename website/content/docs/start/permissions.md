# Permissions

## Workspace trust

Trusted workspace directories are recorded in `CONFIG_DIR/trust.json`. Use `/trust` in the TUI to mark the current workspace. Project-local extensions under `.elph/extensions/` load only after trust.

## Tool policy

Built-in tools (edit, shell, web, …) and MCP tools follow host approval / mode settings. In Plan mode mutating workspace tools need one-shot approval (TUI/ACP); mutating MCP and multi-agent tools stay blocked. Headless Plan denies mutating tools. Allow once does not start Build; many approved writes still mutate the tree. Switching to implementation still requires the confirmation card.

## Logs

| Path                        | Content             |
| --------------------------- | ------------------- |
| `APP_DATA/logs/elph.jsonl`  | App log             |
| `APP_DATA/logs/crash.log-*` | Panic reports       |
| `APP_DATA/logs/mcp/`        | MCP stderr captures |

Skills and prompt templates are instructions for the agent — review third-party skills before enabling them. Bootstrap never overwrites existing user skill files under `CONFIG_DIR/skills/`.
