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

<workDir>/.elph/
├── prompts/*.md             # project templates (override global by filename)
└── logs/
    ├── sess_<id>.log        # session event log (written)
    └── sess_<id>.requests.log  # reserved; not written in production yet
```

## Environment variables

| Variable             | Effect                                                          |
|----------------------|-----------------------------------------------------------------|
| `ELPH_PROVIDERS_DIR` | Replace `~/.elph/providers` (`pkg/ai/provider/paths.go`)        |
| `ELPH_PROMPTS_DIR`   | Replace `~/.elph/prompts` (`internal/prompttemplate/paths.go`)  |
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

| Field                   | Default | Description                                     |
|-------------------------|---------|-------------------------------------------------|
| `theme`                 | `auto`  | `auto`, `dark`, or `light`                      |
| `showThinking`          | `true`  | Stream reasoning blocks in TUI                  |
| `autoExpandThinking`    | `false` | Thinking blocks start expanded                  |
| `thinkingBudgets`       | —       | Per-level token budget overrides                |
| `session.providerId`    | —       | Last selected provider                          |
| `session.modelId`       | —       | Last selected model                             |
| `session.agentMode`     | `build` | `build`, `plan`, `ask`, `brave` (UI only today) |
| `session.thinkingLevel` | `high`  | `off` … `xhigh`                                 |
| `models.lastSync`       | —       | RFC3339 timestamp of last models.dev sync       |
| `models.syncInterval`   | `24h`   | Auto-sync interval in TUI                       |

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

## Project context

| Source                    | Discovery                                            |
|---------------------------|------------------------------------------------------|
| `AGENTS.md`               | Walk up from `workDir` (`internal/prompt/agents.go`) |
| `AGENTS.md` / `CLAUDE.md` | Guardrails block disclosure in system prompt         |

## Session persistence

| Persisted                    | Location                          | Notes                                         |
|------------------------------|-----------------------------------|-----------------------------------------------|
| Provider/model/mode/thinking | `settings.json`                   | Across TUI restarts                           |
| Conversation history         | In-memory `Session.History`       | Provider messages for multi-turn native tools |
| Session log                  | `<workDir>/.elph/logs/sess_*.log` | Append-only event log                         |
| Full chat export             | —                                 | Not implemented                               |

### `--no-session`

Referenced in banner tips but **not implemented** — no CLI flag or ephemeral mode exists yet.

## Related docs

- [cli.md](./cli.md) — `elph provider connect`, `update`, enable/disable
- [prompt-templates.md](./prompt-templates.md) — template directories
- [agent-runtime.md](./agent-runtime.md) — what gets logged per session
