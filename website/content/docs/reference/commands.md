# Slash commands

Type `/` in the prompt to open the command palette. Dispatch order:

1. Built-in commands
2. Extension (WASM) commands
3. Prompt templates (`*.md` under prompts dirs)
4. Skills (by skill name)

## Built-in (selected)

| Command     | Description                              |
| ----------- | ---------------------------------------- |
| `/help`     | List commands                            |
| `/model`    | Open model selector                      |
| `/provider` | List / connect providers                 |
| `/goal`     | Manage session goals                     |
| `/compact`  | Compact conversation history             |
| `/reload`   | Reload extensions and resources          |
| `/commit`   | Commit message helper for staged changes |
| `/trust`    | Trust the current workspace              |
| `/exit`     | Quit                                     |

Skills appear as `/skill-name` when enabled. Prompt templates appear as `/name` (filter with `resources.disabledPrompts` or `resources.prompts` `!` excludes).
