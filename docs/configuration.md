# Configuration

Where Elph stores settings, how to override paths, and what each file controls.

## Directory layout

Default home: `~/.elph/` (override individual dirs with env vars below).

```
~/.elph/
├── settings.json          # UI preferences, session provider/model/mode
└── providers/
    ├── openai.json
    ├── anthropic.json
    └── …                    # one JSON file per provider id

~/.elph/prompts/
└── *.md                     # global prompt templates → /name commands

~/.elph/skills/
└── <name>/SKILL.md          # global agent skills (listed in system prompt)

<workDir>/.agents/elph/
├── .gitignore               # ignores local runtime files (logs, caches)
├── prompts/*.md             # project templates (override global by filename)
├── skills/<name>/SKILL.md   # project skills (override global by name)
└── logs/
    └── <session_id>/
        ├── events.jsonl     # session events (user, system, ai, thinking, shell, …)
        └── requests.jsonl   # provider and tool trace
```

## Environment variables

| Variable             | Effect                                                          |
|----------------------|-----------------------------------------------------------------|
| `ELPH_PROVIDERS_DIR` | Replace `~/.elph/providers` (`pkg/ai/provider/paths.go`)        |
| `ELPH_PROMPTS_DIR`   | Replace `~/.elph/prompts` (`internal/prompttemplate/paths.go`)  |
| `ELPH_SKILLS_DIR`    | Replace `~/.elph/skills` (`internal/prompt/skills.go`)          |
| `ELPH_PROVIDER`      | Force active provider id                                        |
| `ELPH_MODEL`         | Force active model id (can override model on fallback provider) |

Provider JSON files reference API keys via:

- `env.OPENAI_API_KEY`, `$OPENAI_API_KEY`, `${OPENAI_API_KEY}`
- `!shell-command` for command substitution
- Literal strings

Common key env vars: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`.

### CLI env file

```sh
elph --env-file .env.local
```

Loads variables with `gotenv.OverLoad` before any subcommand (`cmd/coding-agent/root.go`).

## `settings.json`

Path: `~/.elph/settings.json` (`internal/settings/settings.go`).

| Field                      | Default   | Description                                                                                                                                                                                          |
|----------------------------|-----------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `theme`                    | `auto`    | `auto`, `dark`, or `light`                                                                                                                                                                           |
| `showThinking`             | `true`    | Stream reasoning blocks in TUI                                                                                                                                                                       |
| `autoExpandThinking`       | `false`   | Thinking blocks start expanded                                                                                                                                                                       |
| `preferedResponseLanguage` | `inherit` | Reply language: `inherit` matches the user's message language; set a fixed language (for example `English`) to always default to that; overridden when the user explicitly asks for another language |
| `thinkingBudgets`          | —         | Per-level token budget overrides                                                                                                                                                                     |
| `session.providerId`       | —         | Last selected provider                                                                                                                                                                               |
| `session.modelId`          | —         | Last selected model                                                                                                                                                                                  |
| `session.agentMode`        | `build`   | `build`, `plan`, `ask`, `brave` — **brave** skips tool approval prompts                                                                                                                              |
| `session.thinkingLevel`    | `high`    | `off` … `xhigh`                                                                                                                                                                                      |
| `models.lastSync`          | —         | RFC3339 timestamp of last models.dev sync                                                                                                                                                            |
| `models.syncInterval`      | `24h`     | Minimum interval before the TUI checks models.dev again at startup                                                                                                                                   |

Model selection priority (`pkg/ai/provider/registry.go`):

1. `ELPH_PROVIDER` + `ELPH_MODEL`
2. Saved `session.providerId` / `modelId`
3. First configured provider with API key and enabled model
4. `ELPH_MODEL` alone when only model env is set

## Provider JSON

One file per provider; **id = filename without `.json`**.

Schema: [schemas/provider-schema.json](../schemas/provider-schema.json).

Supported APIs:

- `openai-completions`
- `anthropic-messages`

Bootstrap templates (`elph provider connect`): OpenAI, Anthropic, OpenCode Zen, OpenCode Go, DeepSeek, Kimi.

Per-model fields include `reasoning`, `thinkingLevelMap`, and `compat` (Pi-style overrides for thinking format, developer role, streaming usage, etc.). See [cli.md](./cli.md) for connect/update/enable commands.

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

Inspect the live prompt in the TUI with `/diagnostic:system-prompt` (collapsible detail box).

## Session persistence

| Persisted                    | Location                                               | Notes                                         |
|------------------------------|--------------------------------------------------------|-----------------------------------------------|
| Provider/model/mode/thinking | `settings.json`                                        | Across TUI restarts                           |
| Conversation history         | In-memory `Session.History`                            | Provider messages for multi-turn native tools |
| Session log                  | `<workDir>/.agents/elph/logs/<sess_id>/events.jsonl`   | Structured JSONL via `slog`                   |
| Requests log                 | `<workDir>/.agents/elph/logs/<sess_id>/requests.jsonl` | Provider/tool trace JSONL                     |
| Full chat export             | —                                                      | Not implemented                               |

### `--no-session`

Referenced in banner tips but **not implemented** — no CLI flag or ephemeral mode exists yet.

## Models.dev sync in the TUI

When `models.syncInterval` has elapsed since `models.lastSync`, the TUI performs **one check at startup** (not on a background timer):

1. Fetches models.dev and runs a **dry-run preview** (`PreviewModelsDevUpdates`) — no provider files are written.
2. If provider files would change, a **[huh](https://github.com/charmbracelet/huh) confirm dialog** asks whether to update (`Update` / `Skip`).
3. If the user chooses **Update**, a full sync runs (`settings.RunModelsSync`), including live `/models` endpoints where configured.
4. If the user chooses **Skip**, or preview finds nothing to change, `models.lastSync` is updated so the prompt does not repeat until the next interval.

To refresh metadata immediately without waiting for the interval, run `elph provider update` from the CLI.

## Related docs

- [cli.md](./cli.md) — `elph provider connect`, `update`, enable/disable
- [prompt-templates.md](./prompt-templates.md) — template directories
- [agent-runtime.md](./agent-runtime.md) — what gets logged per session
