# Slash Commands

Type `/` in the prompt to open the command palette. Dispatch order:

1. Built-in commands
2. Prompt templates (`*.md` under prompts dirs)
3. Skills (by skill name)

## Built-in (selected)

| Command    | Description                              |
| ---------- | ---------------------------------------- |
| `/help`    | List commands                            |
| `/model`   | Open model selector                      |
| `/goal`    | Manage session goals                     |
| `/compact` | Compact conversation history             |
| `/reload`  | Reload hooks and resources               |
| `/commit`  | Commit message helper for staged changes |
| `/exit`    | Quit                                     |

Skills appear as `/skill-name` when enabled. Create new skills with the built-in
[`create-skill`](07-skills.md) skill.

See repo `docs/slash-commands.md` for the full design table.
