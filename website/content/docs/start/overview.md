# Overview

Elph is a local AI coding agent harness. It runs as an interactive TUI (`elph`), as a headless prompt (`elph run`), and as an ACP server for editors (`elph acp --stdio`).

The core harness follows [Pi](https://pi.dev/docs/latest): a small session loop, tools, providers, and a terminal UI you can extend. Elph keeps that model and adds a local project store, floppy memory, worktrees, MCP, WASM extensions, Plan vs Build, and Claude/Codex import.

You bring the model. Sessions and memory stay on disk in the project `.elph/` store. There is no required cloud agent backend.

Elph is **pre-alpha**. Breaking changes and bugs are expected. Check [GitHub Releases](https://github.com/riipandi/elph/releases) before you upgrade.

## Surfaces

| Command            | Role                                                       |
| ------------------ | ---------------------------------------------------------- |
| `elph`             | Interactive TUI — transcript, slash palette, mouse support |
| `elph run "…"`     | Headless prompt for scripts and CI                         |
| `elph acp --stdio` | Agent Client Protocol v1 (add `--experimental` for v2)     |

## What stays local

- Config: `~/.config/elph/` (`ELPH_HOME`)
- Data: `~/.local/share/elph/` (`ELPH_DATA_DIR`)
- Project: `<cwd>/.elph/` — memory (`store.db`). `.elph/extensions/` and `.elph/plans/` are created only when you install a project extension or save a plan. Global `AGENTS.md` is optional and is not written on first run.

## Start here

- [Installation](/docs/start/installation) — install script, cargo, first launch
- [Using Elph](/docs/start/usage) — TUI, resume, headless
- [Providers](/docs/start/providers) — supported catalogs, API keys, OAuth
- [Settings](/docs/start/settings) — paths and environment
- [Sessions](/docs/start/sessions) — resume, store, goals
- [Plan mode](/docs/start/plan-mode) — Plan vs Build
- [Memory](/docs/start/memory) — floppy / project store
- [Permissions](/docs/start/permissions) — trust and tool policy
