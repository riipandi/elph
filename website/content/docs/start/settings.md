# Settings

## Directories

| Role    | Default                | Override           |
| ------- | ---------------------- | ------------------ |
| Config  | `~/.config/elph/`      | `ELPH_HOME`        |
| Data    | `~/.local/share/elph/` | `ELPH_DATA_DIR`    |
| Project | `<cwd>/.elph/`         | `ELPH_PROJECT_DIR` |

| Path                     | Purpose                             |
| ------------------------ | ----------------------------------- |
| `settings.json`          | UI, model defaults, prefs           |
| `auth.json`              | Credentials                         |
| `mcp.json`               | MCP servers                         |
| `providers/*.json`       | Model catalogs                      |
| `skills/<name>/SKILL.md` | User skills                         |
| `bundled/`               | Built-in skills and this user guide |

Project overrides: `<project>/.elph/settings.json`, `mcp.json`, `skills/`, `prompts/`. Project wins on conflict.

`models.defaultModel` seeds **new** sessions only. Live model, thinking level, and agent mode are per-session.

## Environment

| Variable                       | Effect                      |
| ------------------------------ | --------------------------- |
| `ELPH_PROVIDER` / `ELPH_MODEL` | Force provider and model    |
| `ELPH_QUIET`                   | Suppress bootstrap progress |
| `ELPH_LOG_LEVEL`               | `trace` … `error`           |

Inspect what Elph discovered:

```sh
elph doctor
```
