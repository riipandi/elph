# Configuration

Design for file locations, settings merge, and environment overrides.

## Directory layout

Default config: `~/.elph/` | Default data: `~/.local/share/elph/`

```
~/.elph/                                    # XDG_CONFIG_HOME
├── settings.json          # UI and session prefs
├── providers/
│   ├── openai.json
│   ├── anthropic.json
│   └── …                    # one file per provider id
├── prompts/
│   └── *.md                 # global templates → /name
├── extensions/              # global WASM extension bundles
│   └── <name>/
│       ├── extension.toml
│       └── component.wasm
├── extensions.json          # disabled list + extra discovery paths
└── skills/
    └── <name>/SKILL.md      # global skills

~/.local/share/elph/        # XDG_DATA_HOME
├── version.json            # models.dev sync, release metadata
├── metadata.db             # SQLite/Turso — platform sessions
├── attachments/            # pasted images per session
├── models/                 # embedding model cache (memory)
└── logs/

<workDir>/.agents/           # Shared agent (gitignored)
├── prompts/*.md
└── skills/<name>/SKILL.md

<workDir>/.elph/             # Project-local (gitignored)
├── .gitignore
├── settings.json            # optional project overrides
├── store.db                # agent memory (floppy)
├── prompts/*.md
├── extensions/              # project-local WASM bundles (after trust)
│   └── <name>/
├── skills/<name>/SKILL.md
└── metadata/
    └── <session_id>/
        ├── todos.jsonl
        ├── log_events.json
        └── log_requests.json
```

## Environment variables

| Variable               | Effect                                                |
| ---------------------- | ----------------------------------------------------- |
| `ELPH_HOME`            | Override `~/.elph`                                    |
| `ELPH_DATA_DIR`        | Override data directory                               |
| `ELPH_PROJECT_DIR`     | Project root for `.elph/`                             |
| `ELPH_PROVIDERS_DIR`   | Override `providers/`                                 |
| `ELPH_PROMPTS_DIR`     | Override global `prompts/`                            |
| `ELPH_SKILLS_DIR`      | Override global `skills/`                             |
| `ELPH_PROVIDER`        | Force provider id                                     |
| `ELPH_MODEL`           | Force model id                                        |
| `ELPH_PROMPT_ENCODING` | Tool-result prompt encoding: `off`, `toon`, or `auto` |
| `ELPH_PROMPT_ENCODING_MIN_BYTES` | Minimum JSON byte length before TOON encoding applies (default `2048`) |
| `ELPH_PROMPT_ENCODING_DELIMITER` | General TOON delimiter: `comma`, `tab`, or `pipe` (default `comma`) |
| `ELPH_PROMPT_ENCODING_TABULAR_DELIMITER` | Tabular TOON delimiter: `comma`, `tab`, or `pipe` (default `tab`) |
| `ELPH_QUIET`           | Suppress bootstrap output                             |
| `ELPH_TRACE`           | Distributed tracing (`fastrace`): default on; set `0`, `false`, `off`, or `no` to disable |
| `ELPH_LOG_LEVEL`       | Log level: `trace`, `debug`, `info`, `warn`, `error` (default `info`) |
| `ELPH_LOG_FILE`        | Rolling JSONL log file: default on; set `0` to disable |
| `ELPH_LOG_ROTATION`    | Log rotation: `hourly`, `daily` (default), or `weekly` |

Provider JSON may reference API keys via `env.VAR`, `$VAR`, `${VAR}`, `!shell-command`, or literals.

Common keys: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`.

### CLI env file

`--env-file .env.local` loads variables before any subcommand runs.

## JSON

Settings and providers use standard JSON (pretty-printed on save).

## `settings.json`

Schema: [schemas/elph-schema.json](../schemas/elph-schema.json).

Fields are grouped by **domain**. Unknown keys are ignored on load; flat legacy keys (`showThinking`, `scopedModelItems`, …) are migrated into groups on load and rewritten nested on the next save.

### Layered settings

Merge order:

1. Defaults (serde field defaults)
2. `~/.elph/settings.json` (home)
3. `<workDir>/.elph/settings.json` (project), when present

Project overrides **per nested key** (deep merge). Runtime saves write **home only**.

### Domain groups

```json
{
  "ui": {
    "theme": "auto",
    "themes": {
      "dark": { "accent": "#6699ff", "textPrimary": "#d4d5d9" },
      "light": { "accent": "rgb(51, 111, 241)", "codeBlockBg": "#e8eaed" }
    },
    "showThinking": true,
    "autoExpandThinking": false,
    "stickyScroll": true,
    "footerTokenDisplay": "both",
    "coloredStatusFooter": true,
    "filePicker": { "showHiddenFiles": false }
  },
  "session": {
    "agentMode": "build",
    "thinkingLevel": "high"
  },
  "models": {
    "scoped": []
  },
  "provider": {
    "maxRetries": 2,
    "defaultTimeout": "120s"
  },
  "memory": {
    "embedModel": "AllMiniLML6V2",
    "embedQuantized": true
  }
}
```

| Group | Fields | Role |
| ----- | ------ | ---- |
| **`ui`** | `theme`, `themes`, `showThinking`, …, `filePicker.*` | Appearance + transcript / chrome |
| **`session`** | `providerId`, `modelId`, `agentMode`, `thinkingLevel` | Last / preferred session state |
| **`models`** | `scoped` | Ctrl+P cycle + model picker Scoped tab (`/scoped-models`) |
| **`provider`** | `maxRetries`, `defaultTimeout` | LLM HTTP transport defaults |
| **`memory`** | `embedModel`, `embedQuantized` | Floppy / local embeddings |

### Theme (`ui.theme` / `ui.themes`)

| Mode | Behavior |
| ---- | -------- |
| `auto` (default) | Detect terminal via `COLORFGBG` (dark if background ANSI index &lt; 8) |
| `dark` | Built-in Ghostty dark base |
| `light` | Built-in light base |

In the TUI, **Ctrl+Shift+T** rolls `Auto` → `Light` → `Dark` → `Auto`, persists `ui.theme` to home settings, and reinstalls the palette (project `ui.themes.*` overrides still apply).

`ui.themes.dark` / `ui.themes.light` are **partial** token maps. Unset keys keep the base palette.

Supported color forms (→ iocraft `Color::Rgb` / named):

| Form | Example |
| ---- | ------- |
| Hex | `#d4d5d9`, `#fff`, `#6699ffff` |
| CSS | `rgb(102, 153, 255)`, `rgba(255,107,102,0.5)` |
| CSV | `18, 26, 29` |
| Named | `white`, `reset`, `darkgrey`, … |

Token keys (camelCase): `textPrimary`, `textSecondary`, `textMuted`, `textHint`, `accent`, `accentSoft`, `border`, `borderFocus`, `borderSubtle`, `shellBorder`, `shellBorderDimmed`, `surface`, `codeBlockBg`, `selectionBg`, `dialogSelectionBg`, `success`, `warning`, `error`.

## Provider JSON

One file per provider; id = filename without extension.

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json).

Supported APIs: `openai-completions`, `anthropic-messages`.

Bootstrap templates: OpenAI, Anthropic, OpenCode Zen, DeepSeek, Kimi, etc.

Per-model: `reasoning`, `thinkingLevelMap`, `compat` overrides.

## Model selection

Priority:

1. `ELPH_PROVIDER` + `ELPH_MODEL`
2. Merged `session.providerId` / `session.modelId` (project overrides home when set)
3. `ELPH_MODEL` matched across providers

Fresh bootstrap leaves `session.providerId` / `session.modelId` and `models.scoped` **empty** — the TUI shows “No model selected” until the user picks one (`Ctrl+L` / `/model`).

## Project context

| Source           | Discovery                                       |
| ---------------- | ----------------------------------------------- |
| `AGENTS.md`      | Walk up from workDir                            |
| `SKILL.md`       | `~/.elph/skills/` and `<project>/.elph/skills/` |
| Prompt templates | Global and project `prompts/*.md`               |

Live inspection: `/diagnostic:system-prompt`, `/diagnostic:list-tools`.

## Provider catalog refresh

Manual refresh: `elph provider update`.

## Related

- [cli.md](./cli.md) — `provider`, `memory`, `plugin`
- [extensions.md](./extensions.md) — WASM extension paths
- [memory.md](./memory.md) — floppy store
- [agent-runtime.md](./agent-runtime.md) — session logging
