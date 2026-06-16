# Slash Commands

Type `/` in the TUI input to invoke commands. Built-in commands are defined in
`internal/command/builtin.go`. Custom prompt templates from `*.md` files appear as
`/filename` commands unless the name collides with a built-in (built-ins always win).

See [prompt-templates.md](./prompt-templates.md) for template format and arguments.

## Built-in commands

| Command                     | Aliases       | Status              | Description                                                                             |
|-----------------------------|---------------|---------------------|-----------------------------------------------------------------------------------------|
| `/help`                     | —             | **Implemented**     | List all slash commands                                                                 |
| `/model`                    | —             | **Implemented**     | Open model selector (or filter by args)                                                 |
| `/exit`                     | `/quit`, `/q` | **Implemented**     | Quit the application                                                                    |
| `/compact`                  | `/c`          | **Implemented**     | Compact conversation history; optional percentage arg (e.g. `/compact 50`)              |
| `/diagnostic:list-tools`    | —             | **Implemented**     | List agent and diagnostic tools in a collapsible detail box (expanded by default)       |
| `/diagnostic:system-prompt` | —             | **Implemented**     | Show assembled system prompt in a collapsible detail box (collapsed by default)         |
| `/diagnostic:open-log`      | —             | **Implemented**     | Tail session or requests log (`system`, `thinking`, `ai`, `requests`, `thinking_delta`) |
| `/changelog`                | —             | **Not implemented** | Shows placeholder message                                                               |
| `/settings`                 | `/config`     | **Not implemented** | Shows placeholder message                                                               |
| `/diff`                     | —             | **Not implemented** | Shows placeholder message                                                               |
| `/diagnostic:debug`         | —             | **Not implemented** | Shows placeholder message                                                               |

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

| Internal name            | Use instead                 |
|--------------------------|-----------------------------|
| `DiagnosticListTools`    | `/diagnostic:list-tools`    |
| `DiagnosticSystemPrompt` | `/diagnostic:system-prompt` |
| `DiagnosticOpenLog`      | `/diagnostic:open-log`      |

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

## Diagnostic detail boxes

On success, diagnostic slash commands show a **user message** (the slash input) plus a **detail block**
(`internal/command/diagnostic.go` → `Result.DetailLabel` / `DetailBody` / `DetailExpanded`).

| Command                     | Detail label (examples)                   | Default expand |
|-----------------------------|-------------------------------------------|----------------|
| `/diagnostic:list-tools`    | `Available tools`                         | Expanded       |
| `/diagnostic:open-log`      | `Session log (system)`, `Requests log`, … | Expanded       |
| `/diagnostic:system-prompt` | `System prompt`                           | Collapsed      |

`/diagnostic:open-log` args:

| Arg              | Log file            | Filter                     |
|------------------|---------------------|----------------------------|
| `system`         | `log_events.json`   | `[system]` entries         |
| `thinking`       | `log_events.json`   | `[thinking]` entries       |
| `ai`             | `log_events.json`   | `[ai]` entries             |
| `requests`       | `log_requests.json` | Full requests trace        |
| `thinking_delta` | `log_requests.json` | `[thinking_delta]` entries |

Paths: `<workDir>/.agents/elph/metadata/<sess_id>/` — see [configuration.md](./configuration.md).

Expand or collapse any block with `Ctrl+O` or by clicking the header. Usage errors, unknown args,
and missing log paths still appear as normal system notices.

## Related docs

- [prompt-templates.md](./prompt-templates.md)
- [tui.md](./tui.md) — input prompt and keybindings
- [tools.md](./tools.md) — agent tool catalog
