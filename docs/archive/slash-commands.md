# Slash Commands

Design for `/`-prefixed commands in the TUI input.

Dispatch order:

1. **Built-in commands** (this table)
2. **Extension commands** from WASM bundles — see [extensions.md](./extensions.md)
3. **Prompt templates** — `/name` from Markdown files ([prompt-templates.md](./prompt-templates.md))

Built-in commands always win over extension and template names.

## Built-in commands

| Command                     | Aliases       | Description                                                                                                                           |
| --------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `/help`                     | —             | List all commands                                                                                                                     |
| `/model`                    | —             | Open model selector (optional filter args)                                                                                            |
| `/goal`                     | `/goals`      | Manage session goals                                                                                                                  |
| `/exit`                     | `/quit`, `/q` | Quit                                                                                                                                  |
| `/commit`                   | —             | Generate commit message from staged changes                                                                                           |
| `/compact`                  | `/c`          | Compact history; optional percentage arg                                                                                              |
| `/continue`                 | `/cont`       | Resume an interrupted task (submits the recovery prompt; renders as a `Continuing tasks…` meta line, not a user card)                 |
| `/reload`                   | —             | Reload providers (+ disk-only streaming adapters), settings, skills/templates, extensions; conflict notices → sticky transcript cards |
| `/provider`                 | —             | Manage providers: `connect`, `disconnect`, `list`, `update`                                                                           |
| `/diagnostic:list-tools`    | —             | List tools in a detail box                                                                                                            |
| `/diagnostic:system-prompt` | —             | Show assembled system prompt (collapsed default)                                                                                      |
| `/diagnostic:open-log`      | —             | Tail session or requests log                                                                                                          |
| `/changelog`                | —             | Version history (planned)                                                                                                             |
| `/settings`                 | `/config`     | Open settings (planned)                                                                                                               |
| `/diff`                     | —             | Diff view (planned)                                                                                                                   |
| `/diagnostic:debug`         | —             | Debug info (planned)                                                                                                                  |
| `/resume`                   | —             | **Interactive** session picker (empty args); `/resume <id>` switches directly                                                       |
| `/tree`                     | —             | **Interactive** session-tree navigator; `/tree <entry_id> [--summary]` jumps without picker                                           |
| `/import`                   | —             | Import session JSONL into a new session (path required)                                                                               |
| `/export`                   | —             | Export full session tree as JSONL                                                                                                     |
| `/trust`                    | —             | Trust cwd in `CONFIG_DIR/trust.json`                                                                                                  |

### Interactive vs text-only

| Kind | Examples | UX |
| --- | --- | --- |
| Interactive (inline selector / editor) | `/model`, `/resume`, `/tree`, `/rename`, tool approval | Focus status zone; ↑↓ / type / Enter / Esc |
| Scroll text dialog | `/session`, `/tools`, `/system-prompt`, `/settings` | Read-only report; copy optional |
| Status line | `/trust`, `/export`, `/fork` | One-shot result in transcript |

When adding a new slash that needs a choice, open an interactive overlay (`SlashOutcome::OpenItemSelector` or a dedicated dialog) rather than dumping a numbered list for the user to retype.

### `/goal` subcommands

| Subcommand                  | Description                    |
| --------------------------- | ------------------------------ |
| `/goal` / `/goal status`    | Current goal status            |
| `/goal pause`               | Pause active goal              |
| `/goal resume`              | Resume paused or blocked goal  |
| `/goal cancel`              | Remove current goal            |
| `/goal replace <objective>` | Replace with new objective     |
| `/goal next <objective>`    | Queue next goal                |
| `/goal <objective>`         | Create goal from argument text |

Inspired by [Kimi Code CLI slash commands](https://moonshotai.github.io/kimi-code/en/reference/slash-commands.html#autonomous-goal).

### `/provider` subcommands

| Subcommand                  | Description                                                                      |
| --------------------------- | -------------------------------------------------------------------------------- |
| `/provider connect [id]`    | Sign in to a provider (interactive OAuth/API key)                                |
| `/provider disconnect [id]` | Sign out and clear stored credentials                                            |
| `/provider list`            | List configured providers in the transcript                                      |
| `/provider update [id]`     | Update model catalogs from the embedded seed (`~/.config/elph/providers/*.json`) |

`/provider update` applies a **non-destructive merge** by default: your existing catalog file is kept and
only seed models that are missing are added, so custom configuration is preserved. Unparsable files are
left untouched. Conflicts are resolved automatically (no prompt) in the TUI; the CLI offers an
interactive `inquire` selector per conflict (see [cli.md](./cli.md)).

## Extension commands

Extensions contribute slash commands dynamically (e.g. `/say-hello <name>` from the reference bundle). They appear in the autocomplete palette after `/reload` or session start.

| Behavior        | Design                                              |
| --------------- | --------------------------------------------------- |
| Unknown `/foo`  | "not implemented" if no built-in or extension match |
| Extension error | System line: `Extension error: …`                   |
| Success         | System line with extension message                  |

## Prompt templates

`~/.elph/prompts/*.md` and `<project>/.elph/prompts/*.md` map to `/filename`.

On submit:

- Slash input appears as the user message
- Expanded content appears in a collapsible detail block
- Expanded text is sent as the agent turn

## Input prefixes (not slash commands)

| Prefix    | Prompt char | Behavior                    |
| --------- | ----------- | --------------------------- |
| (default) | `>`         | Chat → agent                |
| `/`       | `/`         | Slash command or template   |
| `!`       | `$`         | Shell with agent context    |
| `!!`      | `#`         | Shell without agent context |

## Diagnostic tools vs slash commands

Internal diagnostic names are **not** agent-executable — the UI should redirect to the equivalent slash command.

## Autocomplete

Fuzzy palette when input starts with `/`:

| Key         | Command list                                     | Arg list                 |
| ----------- | ------------------------------------------------ | ------------------------ |
| `Tab` / `→` | Complete command name                            | Cycle arg preview        |
| `↑` / `↓`   | Move selection                                   | Cycle arg preview        |
| `Enter`     | Run if no args needed; else complete to `/name ` | Run with highlighted arg |

`@` mentions: fuzzy workspace file paths (skip `.git`, `node_modules`, etc.).

## Diagnostic detail boxes

| Command                     | Label (examples) | Default expand |
| --------------------------- | ---------------- | -------------- |
| `/diagnostic:list-tools`    | Available tools  | Yes            |
| `/diagnostic:open-log`      | Session log (…)  | Yes            |
| `/diagnostic:system-prompt` | System prompt    | No             |

### `/diagnostic:open-log` args

| Arg              | Log file     | Filter          |
| ---------------- | ------------ | --------------- |
| `system`         | log_events   | `[system]`      |
| `thinking`       | log_events   | `[thinking]`    |
| `ai`             | log_events   | `[ai]`          |
| `requests`       | log_requests | Full trace      |
| `thinking_delta` | log_requests | Thinking deltas |

Paths: `<workDir>/.elph/metadata/<sess_id>/` — see [configuration.md](./configuration.md).

## Related

- [extensions.md](./extensions.md)
- [prompt-templates.md](./prompt-templates.md)
- [tui.md](./tui.md)
- [tools.md](./tools.md)
