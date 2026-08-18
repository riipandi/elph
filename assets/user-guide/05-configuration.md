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

Home settings are merged with project overrides **when the project is trusted** (`/trust` or `trust.defaultProjectTrust: always`). Project wins on conflict; arrays replace. `trust.*` is home-only.

`models.defaultModel` and `models.defaultThinkingLevel` seed **new** sessions only;
live model, thinking level, and agent mode are per-session (not shared settings).

Filter the catalog with `models.enabled` (globs). Filter skills with `resources.disabledSkills` and extra paths in `resources.skills`. Builtin tools: `tools.default`. See repo `docs/settings.md`.

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
| `ELPH_LOG_LEVEL`               | `trace` … `error`           |
| `ELPH_TRACE`                   | Distributed tracing on/off  |

Full layout: repo `docs/configuration.md`.
