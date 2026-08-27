# Configuration

Design for file locations, settings merge, and environment overrides.

## Directory layout

Default config: `~/.config/elph/` (`$XDG_CONFIG_HOME/elph`) · Default data: `~/.local/share/elph/` (`$XDG_DATA_HOME/elph`)

Override with `ELPH_HOME` (config) and `ELPH_DATA_DIR` (data).

```
~/.config/elph/                              # CONFIG_DIR
├── agents/                  # User-managed custom agents (markdown frontmatter)
├── bundled/
│   ├── agents/              # Built-in agents (placeholder dirs)
│   ├── skills/              # Built-in skills (embedded, extracted on first run)
│   │   └── create-skill/SKILL.md
│   ├── user-guide/          # Built-in docs (embedded from repo assets/user-guide)
│   │   ├── README.md
│   │   └── 01-….md …
│   └── manifest.json        # Version + checksums for newly written bundled files
├── extensions/              # Global WASM extension bundles (placeholder / installed)
│   └── <name>/
│       ├── extension.toml
│       └── plugin.wasm
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
├── auth.lock                # Wrapped AES-256 master key (machine-bound)
├── attachments/             # Pasted / uploaded images
├── downloads/               # Downloaded files + update artifacts
├── logs/
│   ├── elph.jsonl           # Rolling app log (logforth; daily rotation)
│   ├── elph-traces.jsonl    # Distributed traces when ELPH_TRACE enabled
│   ├── crash.log-YYYYMMDD   # Panic reports (dated)
│   ├── mcp.log               # MCP client stderr (redirected from fd 2 to keep the TUI clean)
│   └── mcp/                 # MCP server/tool stderr captures
│       └── <MCP_NAME>/
│           └── <TOOL_NAME>.stderr.log
├── mcp_cache/               # Host-level MCP cache (CLI; no session)
├── models/                  # Embedding model cache
├── sessions/                # Session tool-call artifacts (by SESSION_ID)
│   └── <SESSION_ID>/
│       ├── mcp_cache/       # Session MCP response cache
│       ├── terminals/       # Shell / terminal capture files
│       ├── tool_outputs.jsonl
│       └── event_log.jsonl  # Optional diagnostic mirror
├── vendor/
├── worktrees/
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
├── store.db                 # Unified store (Turso) — sessions, goals, memory, transcript
├── plans/plan-*.md
├── prompts/*.md
├── extensions/              # Project-local WASM bundles (after trust)
└── skills/<name>/SKILL.md
```

### Storage roles

| Store                  | Path                                      | Contents                                                                                               |
| ---------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Unified store          | `PROJECT/.elph/store.db`                  | Sessions, goals, agent spawn graph, skill cache, memory, embeddings, transcript cache                  |     |
| Session artifacts      | `APP_DATA/sessions/<SESSION_ID>/`         | `mcp_cache/` (JSONL tool result cache), `terminals/`, `tool_outputs.jsonl`, optional `event_log.jsonl` |
| Host MCP cache         | `APP_DATA/mcp_cache/`                     | CLI MCP ops when no session is active (JSONL tool result cache)                                        |
| App / crash / MCP logs | `APP_DATA/logs/`                          | Rolling JSONL, dated crash logs, MCP stderr                                                            |
| MCP client stderr      | `APP_DATA/logs/mcp.log`                   | Raw fd 2 from the MCP (rmcp) client, redirected out of the TUI (suppression)                           |
| Config files           | `CONFIG_DIR/*.json`                       | Settings, auth, trust, MCP, providers                                                                  |
| Provider catalogs      | `CONFIG_DIR/providers/*.json`             | Disk model overlays (see below)                                                                        |
| Bundled assets         | `CONFIG_DIR/bundled/{user-guide,skills}/` | Embedded in the binary; extracted on bootstrap if missing                                              |

### Bundled user guide and skills

Source tree (repo): `assets/user-guide/`, `assets/skills/<name>/`.

At compile time these files are embedded in the `elph` binary. Bootstrap unpacks them into
`CONFIG_DIR/bundled/` **only when the destination file is missing** (user edits are never
overwritten). Checksums for newly written files are merged into `bundled/manifest.json`.

Skill discovery includes `CONFIG_DIR/bundled/skills` as the lowest-priority directory so
built-ins (e.g. `create-skill`) appear unless a user/project skill overrides the same name.

### Provider catalogs (`CONFIG_DIR/providers/`)

Each `*.json` (except `index.json`) is keyed by file stem as the provider id. Shapes accepted:

1. **Map** — `modelId → model` (embedded unpack shape).
2. **Schema wrapper** — `{ "baseUrl"?, "headers"?, "models": { … } }`. Wrapper `baseUrl` / `headers` are stamped onto models that omit them; per-model values win.

Process-wide registration: `install_provider_catalog_dir` records the directory (and which files are disk-only providers) without parsing anything. `get_builtin_models` / `get_builtin_model` then load `<provider>.json` on first use and merge it over the embedded seed by model `id` (disk wins), caching the result until the directory is re-registered.

**Streaming adapters for disk-only providers:** when a provider id is not built-in, Elph registers a runtime adapter if models use a supported API (`openai-completions`, `openai-responses`, `anthropic-messages`, `google-generative-ai`, `mistral-conversations`, `azure-openai-responses`). Auth resolves from `auth.json` and/or env `PROVIDER_ID_API_KEY` (kebab id → `PROVIDER_ID_API_KEY`). Use `/reload` after editing provider JSON so mid-session catalogs and adapters refresh.

## Environment variables

| Variable                                 | Effect                                                                                    |
| ---------------------------------------- | ----------------------------------------------------------------------------------------- |
| `ELPH_HOME`                              | Override config dir (default `~/.config/elph`)                                            |
| `ELPH_DATA_DIR`                          | Override data directory                                                                   |
| `ELPH_PROJECT_DIR`                       | Project root for `.elph/`                                                                 |
| `ELPH_PROVIDER`                          | Force provider id                                                                         |
| `ELPH_MODEL`                             | Force model id                                                                            |
| `ELPH_PROMPT_ENCODING`                   | Tool-result prompt encoding: `off`, `toon`, or `auto`                                     |
| `ELPH_PROMPT_ENCODING_MIN_BYTES`         | Minimum JSON byte length before TOON encoding applies (default `2048`)                    |
| `ELPH_PROMPT_ENCODING_DELIMITER`         | General TOON delimiter: `comma`, `tab`, or `pipe` (default `comma`)                       |
| `ELPH_PROMPT_ENCODING_TABULAR_DELIMITER` | Tabular TOON delimiter: `comma`, `tab`, or `pipe` (default `tab`)                         |
| `ELPH_QUIET`                             | Suppress bootstrap output                                                                 |
| `ELPH_TRACE`                             | Distributed tracing (`fastrace`): default on; set `0`, `false`, `off`, or `no` to disable |
| `ELPH_LOG_LEVEL`                         | Log level: `trace`, `debug`, `info`, `warn`, `error` (default `info`)                     |
| `ELPH_LOG_FILE`                          | Rolling JSONL log file: default on; set `0` to disable                                    |
| `ELPH_LOG_ROTATION`                      | Log rotation: `hourly`, `daily` (default), or `weekly`                                    |

Provider JSON may reference API keys via `env.VAR`, `$VAR`, `${VAR}`, `!shell-command`, or literals.

Common keys: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`.

### CLI env file

`--env-file .env.local` loads variables before any subcommand runs.

## JSON

Settings and providers use standard JSON (pretty-printed on save).

## `auth.json`

Schema: [schemas/auth-schema.json](../schemas/auth-schema.json) (`https://elph.space/auth-schema.json`).

Logical shape: `{ "$schema"?, "provider": { "<id>": "enc:…" | "env:VAR" }, "mcp": { "<server>": "enc:…" } }`. Field-level `enc:` ciphertext; `env:` refs stay plaintext. Do not commit this file.

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
    "preferredChatLanguage": "english",
    "simplifiedTechnicalEnglish": true,
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
        "density": "compact",
        "filePicker": { "showHiddenFiles": false },
        "allowModeChangeWhileBusy": true,
        "turnStats": true
    },
    // turnStats (default true): a dimmed stats line (duration · tokens in/out/cached ·
    // cost · provider/model) is rendered under the assistant reply after each real
    // agent/chat turn. System operations that produce no AI response — e.g. /compact
    // answering "History is already up to date" — are not shown a stats card.
    // Semantics: docs/design/usage-accounting.md
    "models": {
        "defaultModel": null,
        "sessionTitleModel": "inherit",
        "compactionModel": "inherit",
        "treeBranchSummaries": "inherit",
        "defaultThinkingLevel": "high",
        "showConfiguredOnly": false,
        "scopedModels": [],
        "embed": {
            "model": "AllMiniLML6V2",
            "quantized": true
        }
    },
    "promptEncoding": null,
    "maxRetries": 2,
    "defaultTimeout": "120s",
    "memory": {
        "enabled": true,
        "autoRecall": true,
        "autoCaptureWork": true,
        "autoCaptureExploration": true,
        "topK": 5,
        "contextBudgetChars": 3000,
        "minQueryLength": 15
    },
    "mcp": {
        "cacheTtlSecs": 60,
        "cacheMaxEntries": 2048
    },
    "notifications": {
        "enabled": true,
        "onTurnComplete": true,
        "onToolPermission": true,
        "onUserQuestion": true,
        "onError": true,
        "onTurnCancel": false,
        "onStartupReady": true,
        "minTurnDurationSecs": 5.0,
        "appName": "Elph"
    },
    "compaction": {
        "thresholdPct": 80,
        "keepRecentTokens": 20000
    }
}
```

| Group / field                    | Fields                                                                                                                                                                                                 | Role                                                                                                                                                |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`preferredChatLanguage`**      | (top-level)                                                                                                                                                                                            | Language for user-facing chat prose                                                                                                                 |
| **`simplifiedTechnicalEnglish`** | (top-level, default `true`)                                                                                                                                                                            | Follow Simplified Technical English (ASD-STE100) in every response (chat replies + files written by the agent). `false` → plain style rules only.   |
| **`maxRetries`**                 | (top-level)                                                                                                                                                                                            | LLM HTTP retries on 5xx / network errors                                                                                                            |
| **`defaultTimeout`**             | (top-level)                                                                                                                                                                                            | LLM stream inactivity / SSE stall limit (e.g. `120s`)                                                                                               |
| **`ui`**                         | `theme`, `themes`, `showThinking`, `autoExpandThinking`, `stickyScroll`, `footerTokenDisplay`, `coloredStatusFooter`, `density`, `allowModeChangeWhileBusy`, `turnStats`, `filePicker.showHiddenFiles` | Appearance + transcript / chrome                                                                                                                    |
| **`models`**                     | `defaultModel`, `defaultThinkingLevel`, `sessionTitleModel`, `compactionModel`, `treeBranchSummaries`, `scopedModels`, `showConfiguredOnly`, **`embed`** (`model`, `quantized`)                        | Seeds for **new** sessions + catalog prefs + **local embedding** (floppy memory). **Not** live chat model/mode/thinking                             |
| **`promptEncoding`**             | `mode`, `minBytes`, `minSavingsRatio`, `delimiter`, `tabularDelimiter`, `targets`, `preamble`                                                                                                          | TOON encoding of model-visible tool results (optional; absent/`null` → `ELPH_PROMPT_ENCODING*` env vars)                                            |
| **`memory`**                     | `enabled`, `autoRecall`, `autoCaptureWork`, `autoCaptureExploration`, `topK`, `contextBudgetChars`, `minQueryLength`                                                                                   | Floppy memory hooks + retrieval (see [memory.md](./memory.md)); embed model is under `models.embed`                                                 |
| **`mcp`**                        | `cacheTtlSecs` (default 60), `cacheMaxEntries` (default 2048)                                                                                                                                          | MCP tool result cache retention. Per-server `cacheTtlMs` in `mcp.json` overrides the global TTL. See [mcp.md](./mcp.md).                            |
| **`notifications`**              | `enabled`, `onTurnComplete`, `onToolPermission`, `onUserQuestion`, `onError`, `onTurnCancel`, `onStartupReady`, `minTurnDurationSecs`, `appName`                                                       | Desktop notifications (see [notifications](#notifications-notifications))                                                                           |
| **`compaction`**                 | `thresholdPct`, `keepRecentTokens`                                                                                                                                                                     | Auto-compaction **thresholds** only (auto-compact is always available after turns when usage exceeds the threshold; `/compact` is always available) |

`ui.density` (default `compact`) controls transcript **log density**: `loose` → every process-log row (tool calls, thinking, assistant response) keeps a blank line above and below. `compact` → collapsed tool-call items are packed together into a grouped log (no blank line between consecutive collapsed tool rows); expanded (accessed) tool-call items, `Thinking`, and AI chat response/assistant items always keep line breaks above and below.

Legacy nested `provider: { maxRetries, defaultTimeout }` is lifted to the root on load.

**Per-session state** (active model, thinking level, agent mode) lives on the coding session / Turso session tree so concurrent Elph instances do not race on `settings.json`. New sessions start in agent mode **`build`**. Switching to a model with a smaller context window may auto-compact history so it fits.

### Session titles (`models.sessionTitleModel`)

Sessions get an automatic title after the first user turn, generated in the background by the model in `models.sessionTitleModel` — `"inherit"` (default) uses the session's active model; set it to a `provider/model_id` (e.g. `anthropic/claude-haiku-4-5`) to use a different, usually cheaper, model for naming. Rename manually anytime with `/rename`.

Naming is defensive by design:

- The conversation excerpt keeps the first and most recent user messages (tool results and assistant output are omitted), capped at a small character budget — long sessions don't inflate the naming call.
- Titles are sanitized: quotes, a leading `Title:`/`Session:` label, and trailing punctuation are stripped; generic placeholders (`"Chat"`, `"Conversation"`, …) are rejected.
- If the naming model call fails or returns a generic title, the first user message (truncated to 60 characters) is used as a fallback, so sessions always end up named.
- A failed attempt is retried on later turns (up to 3 tries). An invalid/unknown `sessionTitleModel` ref falls back to the session model instead of skipping naming.

### Prompt encoding (`promptEncoding`)

Optional [TOON](https://github.com/toon-format/toon) encoding compresses large structured JSON in **model-visible** tool results (and MCP `structured_content` details) before the model sees them, reducing input tokens on tabular payloads. See [agent-runtime.md](./agent-runtime.md#toon-prompt-encoding-optional) and [`crates/elph-agent/docs/prompt-encoding.md`](../crates/elph-agent/docs/prompt-encoding.md).

The group is **optional**: when absent or `null`, the agent falls back to the `ELPH_PROMPT_ENCODING*` environment variables (and ultimately `off`). Set the group to override env vars explicitly.

```json
"promptEncoding": {
    "mode": "auto",
    "minBytes": 2048,
    "minSavingsRatio": 1.0,
    "delimiter": "comma",
    "tabularDelimiter": "tab",
    "targets": {
        "toolResultText": true,
        "structuredDetails": true
    },
    "preamble": "Data is in TOON format (2-space indent, arrays show length and fields)."
}
```

| Field              | Type     | Default            | Meaning                                                                                                                 |
| ------------------ | -------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `mode`             | `string` | `"off"`            | `off` (never), `toon` (all eligible payloads), `auto` (uniform tabular arrays only). Unknown values fall back to `off`. |
| `minBytes`         | `int`    | `2048`             | Minimum JSON byte length before encoding applies.                                                                       |
| `minSavingsRatio`  | `number` | `1.0`              | Encode only when TOON is at most this ratio of the JSON size.                                                           |
| `delimiter`        | `string` | `"comma"`          | TOON delimiter for general payloads (`comma` / `tab` / `pipe`).                                                         |
| `tabularDelimiter` | `string` | `"tab"`            | TOON delimiter for tabular arrays.                                                                                      |
| `targets`          | `object` | both `true`        | Which surfaces may be rewritten (`toolResultText`, `structuredDetails`).                                                |
| `preamble`         | `string` | built-in TOON hint | Preamble above TOON fenced blocks.                                                                                      |

### Theme (`ui.theme` / `ui.themes`)

| Mode             | Behavior                                                               |
| ---------------- | ---------------------------------------------------------------------- |
| `auto` (default) | Detect terminal via `COLORFGBG` (dark if background ANSI index &lt; 8) |
| `dark`           | Built-in Ghostty dark base                                             |
| `light`          | Built-in light base                                                    |

In the TUI, **Ctrl+Shift+T** rolls `Auto` → `Light` → `Dark` → `Auto`, persists `ui.theme` to home settings, and reinstalls the palette (project `ui.themes.*` overrides still apply).

`ui.themes.dark` / `ui.themes.light` are **partial** token maps. Unset keys keep the base palette.

Supported color forms (→ iocraft `Color::Rgb` / named):

| Form  | Example                                       |
| ----- | --------------------------------------------- |
| Hex   | `#d4d5d9`, `#fff`, `#6699ffff`                |
| CSS   | `rgb(102, 153, 255)`, `rgba(255,107,102,0.5)` |
| CSV   | `18, 26, 29`                                  |
| Named | `white`, `reset`, `darkgrey`, …               |

Token keys (camelCase): `textPrimary`, `textSecondary`, `textMuted`, `textHint`, `accent`, `accentSoft`, `border`, `borderFocus`, `borderSubtle`, `shellBorder`, `shellBorderDimmed`, `surface`, `codeBlockBg`, `selectionBg`, `dialogSelectionBg`, `success`, `warning`, `error`.

### Desktop notifications (`notifications`)

Optional native OS notifications (macOS Notification Center, Linux D-Bus, Windows Toast). `enabled` is the master switch; individual `on*` flags select which events notify.

| Field                 | Type    | Default  | Meaning                                                        |
| --------------------- | ------- | -------- | -------------------------------------------------------------- |
| `enabled`             | boolean | `true`   | Master switch — disable all desktop notifications.             |
| `onTurnComplete`      | boolean | `true`   | Notify when the agent finishes a turn.                         |
| `onToolPermission`    | boolean | `true`   | Notify when the agent requests tool permission.                |
| `onUserQuestion`      | boolean | `true`   | Notify when the agent asks a question.                         |
| `onError`             | boolean | `true`   | Notify on errors (agent / MCP / bootstrap failure).            |
| `onTurnCancel`        | boolean | `false`  | Notify when a running turn is canceled.                        |
| `onStartupReady`      | boolean | `true`   | Notify when bootstrap / startup completes.                     |
| `minTurnDurationSecs` | number  | `5.0`    | Minimum turn duration (s) before a turn-complete notification. |
| `appName`             | string  | `"Elph"` | App name shown in the notification banner.                     |

## Provider JSON

One file per provider; id = filename without extension.

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json) — map of `modelId →` model (or wrapper `{ "models": … }`). Generated files include `"$schema": "https://elph.space/provider-schema.json"`. Chat entries require `thinkingLevelMap` keys `off|minimal|low|medium|high|xhigh|max`.

Supported APIs (see schema enum): `openai-completions`, `openai-responses`, `openai-codex-responses`, `azure-openai-responses`, `anthropic-messages`, `google-generative-ai`, `google-vertex`, `bedrock-converse-stream`, `mistral-conversations`.

Embedded chat catalogs are generated from **[models.dev](https://models.dev)** via `make generate-models` / skill `update-models`, then compressed into the binary at build time (single self-contained binary, no external data files). On every home bootstrap Elph **unpacks** missing files into `CONFIG_DIR/providers/PROVIDER_ID.json` (kebab-case ids such as `openai`, `anthropic`, `amazon-bedrock`). **Existing files are never overwritten**.

At runtime, `CONFIG_DIR/providers/*.json` is **merged over the embedded catalog** by model `id` (disk wins). Each provider is read and parsed **lazily on first use** and cached for the process; `/reload` (and every bootstrap/session resolve) drops that cache so edited files take effect without a restart. Lists and `resolve_model` honor the merge. Streaming adapters still require a built-in provider id — a pure custom provider file without a matching adapter is logged and skipped for streaming.

Per-model: `reasoning`, `thinkingLevelMap` (required), `compat`, `cost`, `contextWindow`, `maxTokens`, `input`.

## Model selection

Priority for **new** sessions:

1. CLI / env (`ELPH_PROVIDER` + `ELPH_MODEL`, or model override)
2. TUI only — model last used in the project's most recent session (when it still
   exists in the catalog), so a fresh session continues where the previous one left off
3. Merged `models.defaultModel` (`provider/model_id`; project overrides home when set)
4. Provider fallback default when only a provider is known

Fresh bootstrap leaves `models.defaultModel` and `models.scopedModels` **empty** — the TUI shows “No model selected” until the user picks one (`Ctrl+L` / `/model`). Changing the live model in a session does **not** write `defaultModel` (avoids multi-instance conflicts).

## Project context

| Source           | Discovery (last wins on name conflict)                                             |
| ---------------- | ---------------------------------------------------------------------------------- |
| `AGENTS.md`      | Walk up from workDir; bootstrap creates empty `CONFIG_DIR/AGENTS.md` if missing    |
| Custom agents    | `bundled/agents` → `CONFIG_DIR/agents` → project `.agents/agents` → `.elph/agents` |
| `SKILL.md`       | Shared `.agents/skills` → `CONFIG_DIR/skills` → project `.agents` / `.elph` skills |
| Prompt templates | Global and project `prompts/*.md`                                                  |

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

`elph update --check` refreshes `last_checked_at` and the selected channel
version; a successful installation also updates `version`.

Live inspection: `/diagnostic:system-prompt`, `/diagnostic:list-tools`.

## Provider catalog refresh

Manual refresh: `elph provider update`.

## Related

- [cli.md](./cli.md) — `provider`, `memory`, `plugin`
- [extensions.md](./extensions.md) — WASM extension paths
- [memory.md](./memory.md) — floppy store
- [agent-runtime.md](./agent-runtime.md) — session logging
  d) — session logging
