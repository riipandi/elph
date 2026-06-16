---
title: Configuration
summary: "Where Elph stores settings, how to override paths, and what each file controls."
weight: 100
---

Where Elph stores settings, how to override paths, and what each file controls.

## Directory layout

Default home: `~/.elph/` (override individual dirs with env vars below).

```
~/.elph/
├── settings.json          # UI preferences, session provider/model/mode (or settings.jsonc)
├── version.json           # models.dev sync timestamp, release metadata (see below)
└── providers/
    ├── openai.json
    ├── anthropic.json
    └── …                    # one .json or .jsonc file per provider id

~/.elph/prompts/
└── *.md                     # global prompt templates → /name commands

~/.elph/skills/
└── <name>/SKILL.md          # global agent skills (listed in system prompt)

<workDir>/.agents/elph/
├── .gitignore               # ignores metadata/, settings, mcp/, attachments/, and itself; prompts/skills stay committable
├── settings.json            # optional project overrides (or settings.jsonc); merged on load, not written by Save
├── prompts/*.md             # project templates (override global by filename)
├── skills/<name>/SKILL.md   # project skills (override global by name)
├── attachments/             # pasted images per session (gitignored; created on first paste)
└── metadata/
    └── <session_id>/
        ├── todos.jsonl      # latest TodoList snapshot (overwrite, not append)
        ├── log_events.json  # session events (user, system, ai, thinking, shell, …)
        └── log_requests.json # provider and tool trace
```

## Environment variables

| Variable             | Effect                                                                         |
|----------------------|--------------------------------------------------------------------------------|
| `ELPH_PROVIDERS_DIR` | Replace `~/.elph/providers` (`pkg/ai/provider/paths.go`)                       |
| `ELPH_PROMPTS_DIR`   | Replace `~/.elph/prompts` (`internal/prompt/template/paths.go`)                |
| `ELPH_SKILLS_DIR`    | Replace `~/.elph/skills` (`internal/prompt/skills.go`)                         |
| `ELPH_PROVIDER`      | Force active provider id                                                       |
| `ELPH_MODEL`         | Force active model id (matched across providers when `ELPH_PROVIDER` is unset) |

Provider JSON files reference API keys via:

- `env.OPENAI_API_KEY`, `$OPENAI_API_KEY`, `${OPENAI_API_KEY}`
- `!shell-command` for command substitution
- Literal strings

Common key env vars: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`.

### CLI env file

```sh
elph --env-file .env.local
```

Loads variables with `gotenv.OverLoad` before any subcommand (`cmd/elph/root.go`).

## JSON and JSONC

Settings and provider configs accept standard JSON and [JSONC](https://github.com/tidwall/jsonc): `//` and `/* */` comments, plus trailing commas. Use either `.json` or `.jsonc` extensions.

- Settings: `settings.json` is preferred when both `settings.json` and `settings.jsonc` exist. New saves go to whichever file is active (default `settings.json`).
- Providers: one file per provider id; `.json` wins over `.jsonc` when both exist.

Parsing lives in `pkg/jsoncfg`.

## `settings.json`

Schema: [schemas/config-schema.json](../schemas/config-schema.json).

### Layered settings

Elph merges settings from two layers (`internal/settings/load.go`):

1. Defaults
2. `~/.elph/settings.json` or `settings.jsonc` (home)
3. `<workDir>/.agents/elph/settings.json` or `settings.jsonc` (project), when present

Project values override home **field-by-field** for most preferences (theme, `stickyScroll`, `session.agentMode`, and so on).

**Exceptions:**

- `session.providerId` and `session.modelId` from **home always win** when set. Project `session` may only supply default provider/model when home has no selection.
- `Save()` and runtime mutations (`SetActiveModel`, `SetAgentMode`, …) read and write **home only** (`~/.elph`).

On first launch, `settings.Ensure()` creates `~/.elph/settings.json` with defaults if no settings file exists.

### Fields

| Field                      | Default      | Description                                                                                                                                                                                                                 |
|----------------------------|--------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `syncInterval`             | `24h`        | Minimum interval before the TUI checks models.dev again at startup (Go duration, e.g. `24h`, `12h`, `30m`)                                                                                                                  |
| `theme`                    | `auto`       | `auto`, `dark`, or `light`                                                                                                                                                                                                  |
| `showThinking`             | `true`       | Stream reasoning blocks in TUI                                                                                                                                                                                              |
| `autoExpandThinking`       | `false`      | Thinking blocks start expanded                                                                                                                                                                                              |
| `useRawPaste`              | `false`      | When `false`, long text pastes (≥ 4 lines or ≥ 400 runes) collapse to `[Pasted: N lines]` in the input; preview/edit with **Ctrl+O**. When `true`, paste verbatim. See [TUI § Long text paste](/guides/tui/#long-text-paste) |
| `stickyScroll`             | `true`       | Pin the latest user prompt to the top of the viewport while scrolling assistant replies                                                                                                                                     |
| `preferedResponseLanguage` | `inherit`    | Reply language: `inherit` matches the user's message language; set a fixed language (for example `English`) to always default to that; overridden when the user explicitly asks for another language                        |
| `thinkingBudgets`          | —            | Per-level token budget overrides                                                                                                                                                                                            |
| `maxToolIterations`        | `0` (25)     | Max autonomous tool rounds per turn. `0` uses the built-in default (25). Increase if the agent stops prematurely with "Stopped after N tool rounds."                                                                        |
| `autoCompactContext`       | `true`       | Automatically compact conversation history and retry when the provider reports a context-limit error, instead of showing the error to the user                                                                              |
| `autoCompactLimit`         | `80`         | Compaction target as percentage of history budget (10-100). Lower = more aggressive. Used by both auto-compaction and `/compact` slash command                                                                              |
| `footerTokenDisplay`       | `both`      | How token usage is displayed in the footer. Context limit is always shown. `both`: `131k | 0.0% | 262k` (used tokens + percentage); `percentage`: `0.0% | 262k`; `count`: `131k | 262k`                                                                                                       |
| `session.providerId`       | —            | Last selected provider (saved to `~/.elph` on change)                                                                                                                                                                       |
| `session.modelId`          | —            | Last selected model (saved to `~/.elph` on change)                                                                                                                                                                          |
| `session.agentMode`        | `build`      | `build`, `plan`, `ask`, `brave` — **brave** skips tool approval prompts                                                                                                                                                     |
| `session.thinkingLevel`    | `high`       | `off` … `xhigh`                                                                                                                                                                                                             |
Legacy `models.syncInterval` and `models.lastSync` in older settings files are migrated on load (`syncInterval` is promoted to the top level; `lastSync` moves to `version.json`).

### Model selection

Priority (`pkg/ai/provider/registry.go`):

1. `ELPH_PROVIDER` + `ELPH_MODEL`
2. Saved `session.providerId` / `session.modelId` (from merged settings; home selection wins over project defaults)
3. `ELPH_MODEL` matched across configured providers when `ELPH_PROVIDER` is unset

There is **no automatic default model**. Until a model is chosen (or env overrides apply), the TUI shows **No model selected** and blocks chat submit until a provider with a valid `apiKey` is active.

Selecting a model in the picker **always saves** `session.providerId` / `modelId`. If `apiKey` is missing or unresolved, the selection is still persisted and the footer updates, but the runtime provider stays inactive until credentials are configured (see [TUI § Model selector](/guides/tui/#model-selector)).

## Provider JSON

One file per provider; **id = filename without `.json`**.

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json).

Supported APIs:

- `openai-completions`
- `anthropic-messages`

Bootstrap templates (`elph provider connect`): OpenAI, Anthropic, OpenCode Zen, OpenCode Go, DeepSeek, Kimi.

Per-model fields include `reasoning`, `thinkingLevelMap`, and `compat` (Pi-style overrides for thinking format, developer role, streaming usage, etc.). See [CLI](/guides/cli/) for connect/update/enable commands.

## Project context and system prompt

`runtime.Session` builds the provider system prompt once per session via `prompt.Build`
(`internal/prompt/builder.go`). Assembly order:

1. Built-in template (`internal/prompt/template/system.md`) with dynamic tool list
2. `<project_context>` — nearest `AGENTS.md` in `<project_instructions path="…">`
3. `<available_skills>` — skills from `~/.elph/skills` and `<workDir>/.agents/elph/skills` (project overrides global by name)
4. Current date and working directory
5. `<session_state>` — `<session_mode>` from `session.agentMode`
6. Guardrails, thinking instructions, and response language (`preferedResponseLanguage`)
7. Optional additional instructions

Each skill is a directory containing `SKILL.md` with YAML frontmatter (`name`, `description`). The model is instructed to `Read` the skill file when a task matches.

| Source                    | Discovery                                                            |
|---------------------------|----------------------------------------------------------------------|
| `AGENTS.md`               | Walk up from `workDir` (`internal/prompt/agents.go`)                 |
| `SKILL.md`                | `~/.elph/skills/<name>/` and `<workDir>/.agents/elph/skills/<name>/` |
| `AGENTS.md` / `CLAUDE.md` | Guardrails block disclosure in system prompt                         |

Inspect the live prompt with `/diagnostic:system-prompt` (detail box, collapsed by default). List
tools with `/diagnostic:list-tools` or tail logs with `/diagnostic:open-log` (both expanded by default).

## Session persistence

| Persisted             | Location                                     | Notes                                         |
|-----------------------|----------------------------------------------|-----------------------------------------------|
| Provider/model        | `~/.elph/settings.json` → `session.*`        | Home-only save; restored on startup           |
| Agent mode / thinking | `settings.json` (merged)                     | Across TUI restarts                           |
| Conversation history  | In-memory `Session.History`                  | Provider messages for multi-turn native tools |
| Session metadata dir  | `<workDir>/.agents/elph/metadata/<sess_id>/` | Per-session todos + logs                      |
| TodoList snapshot     | `…/metadata/<sess_id>/todos.jsonl`           | Latest task list; removed when empty          |
| Session log           | `…/metadata/<sess_id>/log_events.json`       | Structured JSONL via `slog`                   |
| Requests log          | `…/metadata/<sess_id>/log_requests.json`     | Provider/tool trace JSONL                     |
| Full chat export      | —                                            | Not implemented                               |

### `--no-session`

Referenced in banner tips but **not implemented** — no CLI flag or ephemeral mode exists yet.

## `version.json`

Path: `~/.elph/version.json`. Schema: [schemas/version-schema.json](../schemas/version-schema.json).

| Field               | Description                                                             |
|---------------------|-------------------------------------------------------------------------|
| `lastSyncProviders` | RFC3339 timestamp of the last models.dev provider metadata sync         |
| `relaseCheckedAt`   | Placeholder release-check timestamp (typo preserved in JSON field name) |
| `stableVersion`     | Placeholder stable version string                                       |
| `version`           | Placeholder installed version string                                    |

Legacy `models.lastSync` in old `settings.json` files is migrated to `lastSyncProviders` on first read.

## Models.dev sync in the TUI

When top-level `syncInterval` has elapsed since `version.json` → `lastSyncProviders`, the TUI performs **one check at startup** (not on a background timer):

1. Fetches models.dev and runs a **dry-run preview** (`PreviewModelsDevUpdates`) — no provider files are written.
2. If provider files would change, a **[huh](https://github.com/charmbracelet/huh) confirm dialog** asks whether to update (`Update` / `Skip`).
3. If the user chooses **Update**, a full sync runs (`settings.RunModelsSync`), including live `/models` endpoints where configured.
4. If the user chooses **Skip**, or preview finds nothing to change, `lastSyncProviders` is updated so the prompt does not repeat until the next interval.

To refresh metadata immediately without waiting for the interval, run `elph provider update` from the CLI.

## Related docs

- [CLI](/guides/cli/) — `elph provider connect`, `update`, enable/disable
- [Prompt Templates](/guides/prompt-templates/) — template directories
- [Agent Runtime](/guides/agent-runtime/) — what gets logged per session
