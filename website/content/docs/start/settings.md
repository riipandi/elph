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

Project overrides: `<project>/.elph/settings.json`, `skills/`, `prompts/` always load (project wins; arrays replace). MCP cache: `mcp.json`. Project WASM extensions: `trust.json` (`/trust` or `defaultProjectTrust: always`).

`models.defaultModel` seeds **new** sessions only. Live model, thinking level, and agent mode are per-session.

Catalog filter: `models.enabled` (globs). Skill filter: `resources.disabledSkills`. Builtin tools: `defaultTools`. Full key list: repo `docs/settings.md`.

## Environment

| Variable                       | Effect                      |
| ------------------------------ | --------------------------- |
| `ELPH_PROVIDER` / `ELPH_MODEL` | Force provider and model    |
| `ELPH_QUIET`                   | Suppress bootstrap progress |
| `ELPH_LOG_LEVEL`               | rustlog spec (`info`, or `elph_agent=debug`) |
| `ELPH_LOG_FILE`                | `0` disables rolling JSONL                   |
| `ELPH_LOG_ROTATION`            | `hourly` / `daily` / `size`                  |
| `ELPH_TRACE`                   | Distributed tracing on/off                   |

Inspect what Elph discovered:

```sh
elph doctor
```
