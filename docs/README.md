# Elph Documentation

Index of user and contributor documentation. All docs are written in English.

## Guides

| Document                                     | Description                                        |
|----------------------------------------------|----------------------------------------------------|
| [architecture.md](./architecture.md)         | Repository layout, packages, and data flow         |
| [configuration.md](./configuration.md)       | `~/.elph` paths, settings, env vars, provider JSON |
| [cli.md](./cli.md)                           | `elph` CLI subcommands (`provider`, `version`, …)  |
| [agent-runtime.md](./agent-runtime.md)       | Turn loop, native tools, sessions, logging         |
| [tools.md](./tools.md)                       | Built-in tool catalog and provider API exposure    |
| [slash-commands.md](./slash-commands.md)     | TUI `/` commands — implemented vs planned          |
| [prompt-templates.md](./prompt-templates.md) | Custom `/name` prompt templates                    |
| [tui.md](./tui.md)                           | TUI layout, colors, keybindings                    |
| [progress.md](./progress.md)                 | Development log — agent tools & provider work      |

## JSON schemas

| File                                                               | Purpose                                              |
|--------------------------------------------------------------------|------------------------------------------------------|
| [../schemas/provider-schema.json](../schemas/provider-schema.json) | Provider config format (`~/.elph/providers/*.json`)  |
| [../schemas/config-schema.json](../schemas/config-schema.json)     | General config schema                                |
| [../schemas/mcp-schema.json](../schemas/mcp-schema.json)           | **Planned** MCP config — not consumed by runtime yet |

## Documentation gaps (audit summary)

This section records known mismatches between code and docs as of June 2026.
Prefer code when they disagree until docs or behavior are updated.

### Accurate

- Native tool loop and API filter (Read/Grep/Glob only) — `tools.md`, `progress.md`, `agent-runtime.md`
- Prompt template paths and placeholders — `prompt-templates.md`
- Provider CLI (`connect`, `update`, `list`, enable/disable) — `cli.md`, `configuration.md`
- Git line stats in footer — `tui.md`, `internal/git`

### Fixed in this audit

- `tui.md` keybindings (`Ctrl+A` not `Ctrl+M`; added `Ctrl+L`, `Ctrl+Y`, `Ctrl+Shift+T`)
- `tui.md` `showPromptPrefix` default (`false` in code)
- Stale `notExecutableToolMessage` text in `internal/runtime/tool.go`
- Misleading banner tips in `internal/constants/tips.go`
- README requirements (Go-only build; doc index restored)

### Still placeholder / not implemented

| Area                                                    | Code state                                     | Doc reference               |
|---------------------------------------------------------|------------------------------------------------|-----------------------------|
| MCP                                                     | Banner shows `0/0`; no client                  | `architecture.md`, `tui.md` |
| Banner stats                                            | Hardcoded `0` extensions/commands/skills/tools | `tui.md`                    |
| `/diff`, `/settings`, `/changelog`, `/diagnostic:debug` | `notImplemented` handlers                      | `slash-commands.md`         |
| `elph doctor`                                           | Prints "not yet implemented"                   | `cli.md`                    |
| `--no-session`                                          | Mentioned in tips only; no flag                | `configuration.md`          |
| Requests log                                            | Path reserved; not written in production       | `agent-runtime.md`          |
| Agent modes (`build`/`plan`/`ask`/`brave`)              | UI + settings only; no runtime effect          | `agent-runtime.md`          |
| `internal/datastore`                                    | Empty package stub                             | `architecture.md`           |
| WebSearch, FetchURL, Write, Bash, …                     | Catalog only or needs approval                 | `tools.md`                  |

### Contributing

[CONTRIBUTING.md](../CONTRIBUTING.md) is a generic PR template; local dev workflow is covered in the root [README.md](../README.md) and [cli.md](./cli.md).
