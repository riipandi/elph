# Elph User Guide

Built-in documentation shipped with the binary and unpacked to
`CONFIG_DIR/bundled/user-guide/` on first run (default `~/.config/elph/bundled/user-guide/`).

| Guide                                                     | Topic                                |
| --------------------------------------------------------- | ------------------------------------ |
| [01-getting-started](01-getting-started.md)               | Install, first launch, basic TUI use |
| [02-authentication](02-authentication.md)                 | Provider credentials and env keys    |
| [03-keyboard-shortcuts](03-keyboard-shortcuts.md)         | Common TUI keybindings               |
| [04-slash-commands](04-slash-commands.md)                 | Built-in `/` commands                |
| [05-configuration](05-configuration.md)                   | Paths, settings, env overrides       |
| [06-mcp-servers](06-mcp-servers.md)                       | Model Context Protocol servers       |
| [07-skills](07-skills.md)                                 | Skills and `SKILL.md` packages       |
| [08-custom-models](08-custom-models.md)                   | Provider catalogs and models         |
| [09-sessions](09-sessions.md)                             | Session tree, resume, recovery       |
| [10-memory](10-memory.md)                                 | Project memory (floppy)              |
| [11-plan-mode](11-plan-mode.md)                           | Plan vs Build collaboration mode     |
| [12-subagents](12-subagents.md)                           | Multi-agent collaboration tools      |
| [13-permissions-and-safety](13-permissions-and-safety.md) | Trust, tools, and safety             |

These files are scaffolding. Prefer updating the repo sources under `assets/user-guide/`
and rebuilding; bootstrap never overwrites existing files so local edits are preserved.
