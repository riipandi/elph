# Permissions and Safety

## Workspace trust

Trusted workspace directories are recorded in `CONFIG_DIR/trust.json` (typically
`~/.config/elph/trust.json`). Use `/trust` in the TUI to mark the current
workspace; the path is stored under the `directories` map (absolute or
`~`/`$HOME` forms). Project-local hooks under `.elph/hooks.json` load only after
trust.

## Tool policy

Built-in tools (edit, shell, web, …) and MCP tools follow host approval / mode settings.
In Plan mode, exploration tools run freely; mutating workspace tools require one-shot
approval (no session/all-tools grant). Multi-agent and mutating MCP tools stay hidden
and are blocked if invoked. Headless Plan denies mutating tools (no approval UI).
Allow once does not start Build; many approved writes still mutate the tree.
Switching to implementation still requires the plan confirmation card.

## Logs

| Path                        | Content             |
| --------------------------- | ------------------- |
| `APP_DATA/logs/elph.jsonl`  | App log             |
| `APP_DATA/logs/crash-YYMMDDhh.jsonl` | Panic reports (UTC hour) |
| `APP_DATA/logs/mcp/`        | MCP stderr captures |

## Never auto-run untrusted code

Skills and prompt templates are instructions for the agent — review third-party skills
before enabling them. Bootstrap never overwrites existing user skill files under
`CONFIG_DIR/skills/`; only missing bundled files under `CONFIG_DIR/bundled/` are written.
