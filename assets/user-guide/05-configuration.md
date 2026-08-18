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
| `settings.json`          | UI / model defaults / prefs         |
| `auth.json`              | Credentials                         |
| `mcp.json`               | MCP servers                         |
| `providers/*.json`       | Model catalogs (disk overlay)       |
| `skills/<name>/SKILL.md` | User skills                         |
| `bundled/`               | Built-in agents, skills, user-guide |
| `AGENTS.md`              | Optional global agent instructions (not created on first run) |

Project overrides: `<project>/.elph/settings.json`, `mcp.json`, `skills/`, `prompts/`.

## Settings merge

Home settings are always merged with project `.elph/settings.json` (project wins; arrays replace). MCP cache is in `mcp.json`. Project WASM extensions are gated by `trust.json` (`/trust` or `defaultProjectTrust: always`).

`models.defaultModel` and `models.defaultThinkingLevel` seed **new** sessions only;
live model, thinking level, and agent mode are per-session (not shared settings).

Filter the catalog with `models.enabled` (globs). Filter skills with `resources.disabledSkills`. Builtin tools: `defaultTools`. See repo `docs/settings.md`.

### Transcript log density

`ui.density` controls how tool-call items are spaced in the transcript:

- **`compact`** (default): collapsed tool call items pack together into a grouped log.
  Expanded (accessed) tool call items, `Thinking`, and AI chat response/assistant items always
  keep a line break above and below.
- **`loose`**: every process-log row keeps a blank line above and below.

## Environment

| Variable                       | Effect                      |
| ------------------------------ | --------------------------- |
| `ELPH_PROVIDER` / `ELPH_MODEL` | Force provider/model        |
| `ELPH_QUIET`                   | Suppress bootstrap progress |
| `ELPH_LOG_LEVEL`               | rustlog spec (`info`, or `elph_agent=debug`) |
| `ELPH_LOG_FILE`                | `0` disables `APP_DATA/logs/elph.jsonl`      |
| `ELPH_LOG_ROTATION`            | `hourly` / `daily` / `size`                  |
| `ELPH_TRACE`                   | Distributed tracing on/off                   |

Logs live under `APP_DATA/logs/`: `elph.jsonl`, `elph-traces.jsonl`, `crash-YYMMDDhh.jsonl`, `mcp.log`. Optional `settings.json` `logging` group (restart required); env wins. See repo `docs/settings.md`.

Inspect what Elph discovered for this directory (terminal, clipboard, color, config, auth provider ids, MCP counts, skills, store, logs):

```sh
elph doctor
elph doctor --json   # attach this to a bug report (no secrets)
```
