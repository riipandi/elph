# Permissions and Safety

## Workspace trust

Trusted workspace directories are recorded in `CONFIG_DIR/trust.json`. Project-local
extensions under `.elph/extensions/` load only after trust.

## Tool policy

Built-in tools (edit, shell, web, …) and MCP tools follow host approval / mode settings.
Plan mode restricts the active tool surface.

## Logs

| Path                        | Content             |
| --------------------------- | ------------------- |
| `APP_DATA/logs/elph.jsonl`  | App log             |
| `APP_DATA/logs/crash.log-*` | Panic reports       |
| `APP_DATA/logs/mcp/`        | MCP stderr captures |

## Never auto-run untrusted code

Skills and prompt templates are instructions for the agent — review third-party skills
before enabling them. Bootstrap never overwrites existing user skill files under
`CONFIG_DIR/skills/`; only missing bundled files under `CONFIG_DIR/bundled/` are written.
