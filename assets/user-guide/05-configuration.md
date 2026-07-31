# Configuration

## Directory layout

| Role    | Default path           | Override           |
| ------- | ---------------------- | ------------------ |
| Config  | `~/.config/elph/`      | `ELPH_HOME`        |
| Data    | `~/.local/share/elph/` | `ELPH_DATA_DIR`    |
| Project | `<cwd>/.elph/`         | `ELPH_PROJECT_DIR` |

Important config files:

| Path                     | Purpose                             |
| ------------------------ | ----------------------------------- |
| `settings.json`          | UI / session prefs                  |
| `auth.json`              | Credentials                         |
| `mcp.json`               | MCP servers                         |
| `providers/*.json`       | Model catalogs (disk overlay)       |
| `skills/<name>/SKILL.md` | User skills                         |
| `bundled/`               | Built-in agents, skills, user-guide |
| `AGENTS.md`              | Global agent instructions           |

Project overrides: `<project>/.elph/settings.json`, `mcp.json`, `skills/`, `prompts/`.

## Settings merge

Home settings are merged with project overrides (project wins on conflict). Session model
and thinking preferences live under `settings.session`.

## Environment

| Variable                       | Effect                      |
| ------------------------------ | --------------------------- |
| `ELPH_PROVIDER` / `ELPH_MODEL` | Force provider/model        |
| `ELPH_QUIET`                   | Suppress bootstrap progress |
| `ELPH_LOG_LEVEL`               | `trace` … `error`           |
| `ELPH_TRACE`                   | Distributed tracing on/off  |

Full layout: repo `docs/configuration.md`.
