# Slash Commands

Type `/` in the TUI input to invoke commands. Built-in commands are defined in
`internal/command/builtin.go`. Custom prompt templates from `*.md` files appear as
`/filename` commands unless the name collides with a built-in (built-ins always win).

See [prompt-templates.md](./prompt-templates.md) for template format and arguments.

## Built-in commands

| Command                     | Aliases       | Status              | Description                                              |
|-----------------------------|---------------|---------------------|----------------------------------------------------------|
| `/help`                     | —             | **Implemented**     | List all slash commands                                  |
| `/model`                    | —             | **Implemented**     | Open model selector (or filter by args)                  |
| `/exit`                     | `/quit`, `/q` | **Implemented**     | Quit the application                                     |
| `/diagnostic:list-tools`    | —             | **Implemented**     | List agent and diagnostic tools                          |
| `/diagnostic:system-prompt` | —             | **Implemented**     | Show assembled system prompt in a collapsible detail box |
| `/diagnostic:open-log`      | —             | **Implemented**     | Tail session log (`requests` or `system` arg)            |
| `/changelog`                | —             | **Not implemented** | Shows placeholder message                                |
| `/settings`                 | `/config`     | **Not implemented** | Shows placeholder message                                |
| `/diff`                     | —             | **Not implemented** | Shows placeholder message                                |
| `/diagnostic:debug`         | —             | **Not implemented** | Shows placeholder message                                |

## Prompt templates

Any `~/.elph/prompts/*.md` or `<workDir>/.agents/elph/prompts/*.md` becomes `/name` where
`name` is the filename without `.md`.

On submit:

- The slash input appears as a normal user message
- Expanded prompt content appears in a collapsible detail block
- The expanded text is sent to the agent as the user turn

## Input prefixes (not slash commands)

| Prefix    | Prompt char | Behavior                                      |
|-----------|-------------|-----------------------------------------------|
| (default) | `>`         | Chat message → agent turn                     |
| `/`       | `/`         | Slash command or template                     |
| `!`       | `$`         | Shell with agent context (`runtime.RunShell`) |
| `!!`      | `#`         | Shell without agent context                   |

Leading `/` is stripped on submit for slash commands. `!!` is checked before `!`.

## Diagnostic tools vs slash commands

These internal names are **not** agent-executable (`internal/tools`):

| Internal name              | Use instead                 |
|----------------------------|-----------------------------|
| `diagnostic_list_tools`    | `/diagnostic:list-tools`    |
| `diagnostic_system_prompt` | `/diagnostic:system-prompt` |
| `diagnostic_open_log`      | `/diagnostic:open-log`      |

If the model emits them as text-markup tool calls, the UI shows a message pointing to the slash command.

## Autocomplete

- Slash commands: fuzzy match on name and description (`internal/command/suggest.go`)
- Template args: positional hints from frontmatter `argument-hint`
- `@` mentions: file paths under workspace (`internal/mention`)

### Command palette keys

When the slash palette is open:

| Key         | Command list                                                     | Arg list (commands with `Args` or `argument-hint`) |
|-------------|------------------------------------------------------------------|----------------------------------------------------|
| `Tab` / `→` | Complete selected command name                                   | Cycle argument preview                             |
| `↑` / `↓`   | Move selection                                                   | Cycle argument preview                             |
| `Enter`     | Run if the command needs no args; otherwise complete to `/name ` | Run with the highlighted argument                  |

Examples: `/hel` + `Enter` runs `/help`; `/diagnostic:open-log` + `Enter` runs with the highlighted log target (default `system`); `/identify` + `Enter` completes to `/identify ` and waits for template arguments.

## `/diagnostic:system-prompt` display

On success the TUI shows:

1. **User message** — `/diagnostic:system-prompt`
2. **Detail block** — collapsed by default, labeled `System prompt`, containing the full assembled prompt

Expand with `Ctrl+O` or by clicking the block header. Errors (empty prompt) still appear as a normal system notice.

## Related docs

- [prompt-templates.md](./prompt-templates.md)
- [tui.md](./tui.md) — input prompt and keybindings
- [tools.md](./tools.md) — agent tool catalog
