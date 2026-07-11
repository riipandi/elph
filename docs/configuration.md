# Configuration

Design for file locations, settings merge, and environment overrides.

## Directory layout

Default config: `~/.elph/` | Default data: `~/.local/share/elph/`

```
~/.elph/                                    # XDG_CONFIG_HOME
├── settings.json          # UI and session prefs (or settings.jsonc)
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
| `ELPH_QUIET`           | Suppress bootstrap output                             |

Provider JSON may reference API keys via `env.VAR`, `$VAR`, `${VAR}`, `!shell-command`, or literals.

Common keys: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`.

### CLI env file

`--env-file .env.local` loads variables before any subcommand runs.

## JSON and JSONC

Settings and providers accept standard JSON and JSONC (`//`, `/* */`, trailing commas).

- Settings: prefer `settings.json` when both `.json` and `.jsonc` exist; saves go to the active file.
- Providers: one file per id; `.json` wins over `.jsonc`.

## `settings.json`

Schema: [schemas/elph-schema.json](../schemas/elph-schema.json).

### Layered settings

Merge order:

1. Defaults
2. `~/.elph/settings.json` (home)
3. `<workDir>/.elph/settings.json` (project), when present

Project overrides **per field** for UI preferences.

**Exceptions:**

- `session.providerId` / `session.modelId` from **home always win** when set
- Runtime saves write **home only**

### Main fields

| Field                      | Default         | Description                              |
| -------------------------- | --------------- | ---------------------------------------- |
| `syncInterval`             | `24h`           | models.dev check interval at TUI startup |
| `theme`                    | `auto`          | `auto` / `dark` / `light`                |
| `showThinking`             | `true`          | Stream reasoning in TUI                  |
| `autoExpandThinking`       | `false`         | Thinking blocks start expanded           |
| `useRawPaste`              | `false`         | Collapse long paste in input             |
| `stickyScroll`             | `true`          | Pin user prompt while scrolling replies  |
| `preferedResponseLanguage` | `inherit`       | Reply language hint                      |
| `maxToolIterations`        | `0` (25)        | Max tool rounds per turn                 |
| `autoCompactContext`       | `true`          | Auto-compact on context overflow         |
| `autoCompactLimit`         | `80`            | Compaction target % of history budget    |
| `footerTokenDisplay`       | `both`          | `both` / `percentage` / `count`          |
| `session.providerId`       | —               | Last provider                            |
| `session.modelId`          | —               | Last model                               |
| `session.agentMode`        | `build`         | `build` / `plan` / `ask` / `brave`       |
| `session.thinkingLevel`    | `high`          | `off` … `xhigh`                          |
| `database.url`             | —               | Turso URL (default: local metadata.db)   |
| `database.token`           | —               | Turso cloud token                        |
| `memory.embedModel`        | `AllMiniLML6V2` | Embedding model for floppy               |
| `memory.embedQuantized`    | `true`          | Prefer quantized ONNX variant            |

### Provider HTTP

| Setting                   | Default | Description                     |
| ------------------------- | ------- | ------------------------------- |
| `provider.maxRetries`     | `2`     | Retries on 5xx / network errors |
| `provider.defaultTimeout` | `120s`  | Inactivity and SSE stall limit  |

## Provider JSON

One file per provider; id = filename without extension.

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json).

Supported APIs: `openai-completions`, `anthropic-messages`.

Bootstrap templates: OpenAI, Anthropic, OpenCode Zen, DeepSeek, Kimi, etc.

Per-model: `reasoning`, `thinkingLevelMap`, `compat` overrides.

## Model selection

Priority:

1. `ELPH_PROVIDER` + `ELPH_MODEL`
2. Saved `session.*` (home wins over project)
3. `ELPH_MODEL` matched across providers

**No automatic default model** — TUI shows "No model selected" until the user picks one.

## Project context

| Source           | Discovery                                       |
| ---------------- | ----------------------------------------------- |
| `AGENTS.md`      | Walk up from workDir                            |
| `SKILL.md`       | `~/.elph/skills/` and `<project>/.elph/skills/` |
| Prompt templates | Global and project `prompts/*.md`               |

Live inspection: `/diagnostic:system-prompt`, `/diagnostic:list-tools`.

## Models.dev sync (TUI)

When `syncInterval` has elapsed since `version.json` → `lastSyncProviders`:

1. One check at TUI startup (not a background timer)
2. Dry-run preview
3. Confirm dialog: Update / Skip
4. Skip or no changes → still advance timestamp

Immediate refresh: `elph provider update`.

## Related

- [cli.md](./cli.md) — `provider`, `memory`, `plugin`
- [extensions.md](./extensions.md) — WASM extension paths
- [memory.md](./memory.md) — floppy store
- [agent-runtime.md](./agent-runtime.md) — session logging
