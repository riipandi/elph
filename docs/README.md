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

| File                                                               | Purpose                                                            |
|--------------------------------------------------------------------|--------------------------------------------------------------------|
| [../schemas/provider-schema.json](../schemas/provider-schema.json) | Provider config format (`~/.elph/providers/*.json`)                |
| [../schemas/config-schema.json](../schemas/config-schema.json)     | Settings (`~/.elph` + optional `<workDir>/.agents/elph` overrides) |
| [../schemas/version-schema.json](../schemas/version-schema.json)   | `~/.elph/version.json` (sync timestamp, release metadata)          |
| [../schemas/mcp-schema.json](../schemas/mcp-schema.json)           | **Planned** MCP config — not consumed by runtime yet               |

## Documentation gaps (audit summary)

This section records known mismatches between code and docs as of June 2026.
Prefer code when they disagree until docs or behavior are updated.

### Accurate

- Native tool loop and API filter (Read, Write, Edit, Grep, Glob, ReadMediaFile, WebSearch, AskUser, Bash, TodoList, Skill, CreateGoal, GetGoal, UpdateGoal, SetGoalBudget) — `tools.md`, `progress.md`, `agent-runtime.md`
- Goal tools lifecycle and session-scoped state manager — `tools.md`, `agent-runtime.md`, `pkg/tools/goal`
- Read line_offset/n_lines, Write mode, Edit no-op guard, Grep context_lines, Bash cwd/timeout, WebSearch limit/include_content — `tools.md`
- TodoList Tasks panel, per-session `metadata/<sess_id>/todos.jsonl`, completion notice — `tools.md`, `tui.md`, `agent-runtime.md`
- ReadMediaFile execution, user vision paste (Ctrl/Cmd+V), attachment shortcuts — `tools.md`, `tui.md`, `agent-runtime.md`
- Long text paste collapse, paste editor (Ctrl+O), `useRawPaste` — `tui.md`, `configuration.md`, `progress.md`
- AI prose reflow, Glamour v2 markdown (tables, blockquotes, preprocess), copy hint (`Ctrl+Y` / click) — `tui.md`, `architecture.md`, `agent-runtime.md`, `progress.md` §19
- Write/Edit/Bash approval (huh), streaming shell output, deny cache — `tools.md`, `tui.md`, `agent-runtime.md`
- Native tool detail expand rules (shell expanded; long non-shell collapsed) — `tui.md`
- Prompt template paths and placeholders — `prompt-templates.md`
- System prompt assembly, skills paths, `preferedResponseLanguage` — `configuration.md`, `agent-runtime.md`
- Slash palette `Enter` behavior and diagnostic detail boxes — `slash-commands.md`, `tui.md`
- Project runtime paths (`<workDir>/.agents/elph/metadata/<sess_id>/`, todos + `log_*.json`) — `configuration.md`, `agent-runtime.md`
- Provider CLI (`connect`, `update`, `list`, enable/disable) — `cli.md`, `configuration.md`
- Git footer (lazy branch refresh + on-demand line stats) — `tui.md`, `architecture.md`, `internal/git`
- Memory limits and idle RSS — `architecture.md`, `agent-runtime.md`, `progress.md`
- models.dev startup check + huh confirm — `configuration.md`, `tui.md`, `cli.md`

### Fixed in this audit

- Session metadata layout (`metadata/<sess_id>/todos.jsonl`, `log_events.json`, `log_requests.json`) — `configuration.md`, `agent-runtime.md`, `slash-commands.md`
- `pkg/tools` package layout (`catalog/`, `exposure/`, `schema/`, `goal/`, `todolist/`, `websearch/`) — `tools.md`, `architecture.md`, `AGENTS.md`
- `tui.md` keybindings (`Ctrl+A` not `Ctrl+M`; added `Ctrl+L`, `Ctrl+Y`, `Ctrl+Shift+T`)
- `tui.md` `showPromptPrefix` default (`false` in code)
- Stale `notExecutableToolMessage` text in `internal/runtime/tool.go`
- Misleading banner tips in `internal/constants/tips.go`
- README requirements (Go-only build; doc index restored)

### Still placeholder / not implemented

| Area                                                    | Code state                                                                                   | Doc reference                  |
|---------------------------------------------------------|----------------------------------------------------------------------------------------------|--------------------------------|
| MCP                                                     | Banner shows `0/0`; no client                                                                | `architecture.md`, `tui.md`    |
| Banner stats                                            | Hardcoded `0` extensions/commands/skills/tools (skills are discovered for the system prompt) | `tui.md`                       |
| `/diff`, `/settings`, `/changelog`, `/diagnostic:debug` | `notImplemented` handlers                                                                    | `slash-commands.md`            |
| `elph doctor`                                           | Prints "not yet implemented"                                                                 | `cli.md`                       |
| `--no-session`                                          | Mentioned in tips only; no flag                                                              | `configuration.md`             |
| Agent modes (`build`/`plan`/`ask`)                      | UI + settings only; no runtime effect yet                                                    | `agent-runtime.md`             |
| Agent mode **brave**                                    | Skips tool approval (`SkipToolApproval`)                                                     | `agent-runtime.md`, `tools.md` |
| `internal/datastore`                                    | Empty package stub                                                                           | `architecture.md`              |
| Agent mode build/plan/ask runtime effect                | No runtime difference yet (only UI)                                                          | `agent-runtime.md`             |

### Contributing

[CONTRIBUTING.md](../CONTRIBUTING.md) is a generic PR template; local dev workflow is covered in the root [README.md](../README.md) and [cli.md](./cli.md).
