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

Project overrides: `<project>/.elph/settings.json`, `hooks.json`, `skills/`,
`prompts/` always load (project wins; arrays replace). MCP cache: `mcp.json`.
Project hooks are gated by `trust.json`; interactive TUI startup prompts for
untrusted projects when `defaultProjectTrust` is `ask`.

`models.defaultModel` seeds **new** sessions only. Live model, thinking level, and agent mode are per-session.

## Prompt and resource settings

`ui.atomicPaste` defaults to `true`. Long clipboard text (at least four lines or 400 runes) is stored in `APP_DATA/temp/` and shown as an atomic `[Paste#N: N lines]` marker. Cursor movement, backspace, delete, and selection treat the marker as one unit. A preview appears above the textarea only while the caret touches the marker; **Enter** or **Ctrl+O** expands it. Set it to `false` to insert long clipboard text normally.

`resources.skills` and `resources.prompts` support extra paths and ordered filters. `!` excludes by name or path, `-` uses exact matching for names (path entries use path matching), and `+` re-includes a matching resource. `~` expands to the home directory; relative paths are resolved from the project directory during workspace discovery. For example, this keeps project skills and explicitly selected user skills while excluding the rest of `~/.agents/skills/`:

```json
{
  "ui": {
    "atomicPaste": true
  },
  "resources": {
    "skills": [
      ".agents/skills",
      "!~/.agents/skills/*",
      "~/.agents/skills/commit-only",
      "~/.agents/skills/identify"
    ]
  }
}
```

`resources.disabledSkills` and `resources.disabledPrompts` are name filters applied after discovery. Builtin tools: `defaultTools`. Full key list: repo `docs/settings.md`.

## Clipboard attachments

Press **Ctrl+V** (or **Cmd+V** on macOS) to paste from the clipboard. Image staging runs in the background, so the prompt stays interactive while an ephemeral loading notification is shown. Images appear as atomic `[Image #N]` markers; a full-width preview dialog appears above the textarea only when the caret touches the marker. Images are stored as temporary PNG files in `APP_DATA/attachments/` and require a vision-capable model.

JPEG/JPG, Bitmap/DIB, and other raster images may be accepted when the clipboard exposes decodable image data, but Elph normalizes them to PNG. SVG is not treated as a vector attachment and may fall back to ordinary text when copied as SVG text or HTML. Staged attachments are removed after submission or when discarded, and an existing filename receives a numeric suffix instead of being overwritten.

## Environment

| Variable                       | Effect                      |
| ------------------------------ | --------------------------- |
| `ELPH_PROVIDER` / `ELPH_MODEL` | Force provider and model    |
| `ELPH_QUIET`                   | Suppress bootstrap progress |
| `ELPH_LOG_LEVEL`               | rustlog spec (`info`, or `elph_agent=debug`) |
| `ELPH_LOG_FILE`                | `0` disables rolling JSONL                   |
| `ELPH_LOG_ROTATION`            | `hourly` / `daily` / `size`                  |
| `ELPH_TRACE`                   | Distributed tracing on/off                   |

Inspect what Elph discovered (paths, settings, auth provider ids, MCP counts, skills, store, logs). Attach `--json` to bug reports — never `auth.json`.

```sh
elph doctor
elph doctor --json
```
