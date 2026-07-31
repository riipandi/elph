# Configuration

Design for file locations, settings merge, and environment overrides.

## Directory layout

Default config: `~/.config/elph/` (`$XDG_CONFIG_HOME/elph`) · Default data: `~/.local/share/elph/` (`$XDG_DATA_HOME/elph`)

Override with `ELPH_HOME` (config) and `ELPH_DATA_DIR` (data).

```
~/.config/elph/                              # CONFIG_DIR
├── agents/                  # User-managed custom agents (markdown frontmatter)
├── bundled/
│   ├── agents/              # Built-in agents
│   ├── skills/
│   ├── user-guide/
│   ├── personas/
│   └── manifest.json        # Checksums for bundled content
├── extensions/              # Global WASM extension bundles (placeholder / installed)
│   └── <name>/
│       ├── extension.toml
│       └── component.wasm
├── hooks/                   # User hooks
├── prompts/
│   └── *.md                 # Global templates → /name
├── providers/
│   ├── openai.json
│   ├── anthropic.json
│   └── …                    # One file per provider id (kebab-case)
├── skills/
│   └── <name>/SKILL.md      # User-managed skills
├── AGENTS.md                # Global agent instructions
├── auth.json                # Provider + MCP credentials
├── mcp.json                 # MCP server config
├── settings.json            # UI and session prefs
└── trust.json               # Trusted workspace directories

~/.local/share/elph/                         # APP_DATA
├── attachments/             # Pasted / uploaded images
├── downloads/               # Downloaded files + update artifacts
├── logs/
│   ├── elph.jsonl           # Rolling app log (logforth; daily rotation)
│   ├── elph-traces.jsonl    # Distributed traces when ELPH_TRACE enabled
│   ├── crash.log-YYYYMMDD   # Panic reports (dated)
│   └── mcp/                 # MCP server/tool stderr captures
│       └── <MCP_NAME>/
│           └── <TOOL_NAME>.stderr.log
├── mcp_cache/               # Host-level MCP cache (CLI; no session)
├── models/                  # Embedding model cache
├── projects/                # Session tool-call artifacts (by SESSION_ID)
│   └── <SESSION_ID>/
│       ├── mcp_cache/       # Session MCP response cache
│       ├── terminals/       # Shell / terminal capture files
│       ├── tool_outputs.jsonl
│       └── event_log.jsonl  # Optional diagnostic mirror
├── sessions/                # Legacy SessionDir root (library hosts only)
├── vendor/
├── worktrees/
├── metadata.db              # Turso — sessions tree, goals, spawn graph, …
├── version.json
├── CHANGELOG.md
└── CHANGELOG.json

<workDir>/.agents/           # Shared agent conventions (gitignored)
├── prompts/*.md
└── skills/<name>/SKILL.md

<workDir>/.elph/             # Project-local (gitignored)
├── .gitignore
├── settings.json            # Optional project overrides
├── mcp.json
├── store.db                 # Floppy memory (Turso)
├── metadata.db              # TUI transcript archive only
├── plans/plan-*.md
├── prompts/*.md
├── extensions/              # Project-local WASM bundles (after trust)
└── skills/<name>/SKILL.md
```

### Storage roles

| Store | Path | Contents |
| ----- | ---- | -------- |
| Platform DB | `APP_DATA/metadata.db` | Goals, agent spawn graph, skill cache, session index + tree |
| Floppy memory | `PROJECT/.elph/store.db` | Agent long-term memory / embeddings |
| Transcript archive | `PROJECT/.elph/metadata.db` | TUI card overflow only (not the LLM session tree) |
| Session artifacts | `APP_DATA/projects/<SESSION_ID>/` | `mcp_cache/`, `terminals/`, `tool_outputs.jsonl`, optional `event_log.jsonl` |
| Host MCP cache | `APP_DATA/mcp_cache/` | CLI MCP ops when no session is active |
| App / crash / MCP logs | `APP_DATA/logs/` | Rolling JSONL, dated crash logs, MCP stderr |
| Config files | `CONFIG_DIR/*.json` | Settings, auth, trust, MCP, providers |

Goals remain on `APP_DATA/metadata.db` (`goals` table). Path and table contract must stay stable across layout refactors.

## Environment variables

| Variable               | Effect                                                |
| ---------------------- | ----------------------------------------------------- |
| `ELPH_HOME`            | Override config dir (default `~/.config/elph`)        |
| `ELPH_DATA_DIR`        | Override data directory                               |
| `ELPH_PROJECT_DIR`     | Project root for `.elph/`                             |
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
2. `CONFIG_DIR/settings.json` (home, default `~/.config/elph/settings.json`)
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
    "scoped": [],
    "showConfiguredOnly": true
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
| **`models`** | `scoped`, `showConfiguredOnly` | Ctrl+P cycle + model picker Scoped tab; filter All/Provider tabs to auth-configured providers (default `true`) |
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

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json) — full model shape aligned with `crates/elph-ai/models/*.json` (including required `thinkingLevelMap` with keys `off|minimal|low|medium|high|xhigh|max`).

Supported APIs (see schema enum): `openai-completions`, `openai-responses`, `openai-codex-responses`, `azure-openai-responses`, `anthropic-messages`, `google-generative-ai`, `google-vertex`, `bedrock-converse-stream`, `mistral-conversations`.

Embedded chat catalogs are generated from **[models.dev](https://models.dev)** via `make generate-models` / skill `update-models`. On every home bootstrap Elph **unpacks** missing files into `CONFIG_DIR/providers/PROVIDER_ID.json` (kebab-case ids such as `openai`, `anthropic`, `amazon-bedrock`). **Existing files are never overwritten**.

At runtime, `CONFIG_DIR/providers/*.json` is **merged over the embedded catalog** by model `id` (disk wins). Lists and `resolve_model` honor the merge. Streaming adapters still require a built-in provider id — a pure custom provider file without a matching adapter is logged and skipped for streaming.

Per-model: `reasoning`, `thinkingLevelMap` (required), `compat`, `cost`, `contextWindow`, `maxTokens`, `input`.

## Model selection

Priority:

1. `ELPH_PROVIDER` + `ELPH_MODEL`
2. Merged `session.providerId` / `session.modelId` (project overrides home when set)
3. `ELPH_MODEL` matched across providers

Fresh bootstrap leaves `session.providerId` / `session.modelId` and `models.scoped` **empty** — the TUI shows “No model selected” until the user picks one (`Ctrl+L` / `/model`).

## Project context

| Source           | Discovery (last wins on name conflict)                                              |
| ---------------- | ----------------------------------------------------------------------------------- |
| `AGENTS.md`      | Walk up from workDir; bootstrap creates empty `CONFIG_DIR/AGENTS.md` if missing     |
| Custom agents    | `bundled/agents` → `CONFIG_DIR/agents` → project `.agents/agents` → `.elph/agents`  |
| `SKILL.md`       | Shared `.agents/skills` → `CONFIG_DIR/skills` → project `.agents` / `.elph` skills  |
| Prompt templates | Global and project `prompts/*.md`                                                   |

### Custom agents

Markdown files with optional YAML frontmatter. Supported layouts:

- `agents/<name>.md`
- `agents/<name>/AGENT.md` (or `agent.md`)

```markdown
---
name: reviewer
description: Review code changes
tools:
  - read
  - grep
model: anthropic/claude-sonnet-4
---

You are a careful code reviewer. Focus on correctness and security.
```

## `trust.json`

```json
{
  "directories": {
    "~/Developer/Experimental": true,
    "$HOME/Developer/github.com": true,
    "/Users/johndoe/gitlab.com": true
  }
}
```

Empty default: `{ "directories": {} }`.

## `version.json`

```json
{
  "version": "0.2.114",
  "stable_version": "0.2.114",
  "canary_version": "0.2.114",
  "last_checked_at": "2026-07-29T10:41:42.215675Z"
}
```

Live inspection: `/diagnostic:system-prompt`, `/diagnostic:list-tools`.

## Provider catalog refresh

Manual refresh: `elph provider update`.

## Related

- [cli.md](./cli.md) — `provider`, `memory`, `plugin`
- [extensions.md](./extensions.md) — WASM extension paths
- [memory.md](./memory.md) — floppy store
- [agent-runtime.md](./agent-runtime.md) — session logging
