# Owly

Owly is a terminal CLI that generates and maintains wikis for **personal knowledge** and **code repositories**, built on [elph-agent](https://github.com/riipandi/elph/tree/main/crates/elph-agent) and [elph-ai](https://github.com/riipandi/elph/tree/main/crates/elph-ai). It is a Rust port of [OpenWiki](https://github.com/langchain-ai/openwiki).

## Install

```sh
cargo install --locked owly
```

Or from this repo:

```sh
cargo install --path owly
# or
cargo run -p owly -- [OPTIONS] [MESSAGE]
```

## Modes

| Mode | Wiki location | Default |
|------|---------------|---------|
| **personal** | `~/.owly/wiki/` | yes |
| **code** | `./openwiki/` in the repository | explicit |

Personal mode is the default when no mode is given. Code mode targets repository documentation and optional `AGENTS.md` / `CLAUDE.md` refresh blocks.

```sh
owly "What is in my wiki?"              # personal chat (streams by default)
owly personal --init                    # bootstrap personal wiki
owly code --init                        # bootstrap repo openwiki/
owly --mode code --update               # update repo docs
```

## Quick start

**Personal brain**

```sh
owly personal --init
owly personal --update
owly "Summarize my commitments"
```

**Repository docs**

```sh
owly code --init
owly code --update
owly code "Document the API layer"
```

Configure provider credentials interactively on first run, or set keys in `~/.owly/.env` (see [Configuration](#configuration)).

## Usage

### Chat

```sh
owly "Siapa kamu?"                      # stream assistant text to stdout
owly -p "Summarize what Owly can do"    # print final output only
owly -v "Explain your tools"            # stream + thinking on stderr
owly --stream -p "..."                  # force stream even with --print
```

Bare `owly` (no args) prints `Interactive mode not yet implemented` until the REPL ships.

### Init / update

`--init` requires an explicit mode:

```sh
owly personal --init
owly code --init
```

Flags may follow the mode token (clap `trailing_var_arg` recovery):

```sh
owly personal --init --dry-run
owly code --update -p
```

### Plan without LLM

```sh
owly --dry-run personal --init
owly --dry-run code --update
owly --dry-run "hello"
```

### Diagnostics

```sh
owly --credentials    # masked key / OAuth status
owly --help
```

### Connectors (personal mode)

```sh
owly auth list
owly auth configure git-repo
owly auth configure web-search
owly auth configure hackernews
owly ingest all
owly ingest web-search
owly cron list
owly cron pause web-search
```

**Not supported in Owly:** `ngrok`, `auth tools`, Slack/Gmail/Notion OAuth, ChatGPT subscription OAuth, LangSmith tracing UI.

### Code-mode extras

After a successful docs write in code mode, Owly refreshes `AGENTS.md` / `CLAUDE.md` (`OWLY:START`/`END` blocks) and may create `.github/workflows/owly-update.yml` once.

CI examples: [`examples/`](./examples/) (GitHub Actions, GitLab CI, Bitbucket Pipelines).

## Configuration

### Environment

| Variable | Description | Default |
|----------|-------------|---------|
| `OWLY_PROVIDER` | LLM provider | `opencode` |
| `OWLY_MODEL_ID` | Model id | `big-pickle` |

### Provider API keys

| Provider | Environment variable |
|----------|---------------------|
| OpenCode | `OPENCODE_API_KEY` |
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |
| Google | `GOOGLE_API_KEY` |
| DeepSeek | `DEEPSEEK_API_KEY` |
| Groq | `GROQ_API_KEY` |
| Fireworks | `FIREWORKS_API_KEY` |
| Together | `TOGETHER_API_KEY` |
| Mistral | `MISTRAL_API_KEY` |

### Files under `~/.owly/`

| Path | Purpose |
|------|---------|
| `.env` | Provider keys and defaults |
| `wiki/` | Personal wiki root |
| `owly.sqlite` | Chat session checkpoints (Turso) |
| `onboarding.json` | Personal onboarding + connector instances |
| `INSTRUCTIONS.md` | Personal wiki goals |

## Documentation layout (code mode)

```
openwiki/
├── quickstart.md
├── .last-update.json
├── architecture/
├── workflows/
├── domain/
├── api/
├── operations/
├── integrations/
└── testing/
```

## Tools

**Chat (read-only):** `read`, `grep`, `find`, `ls`, `ask_text`, `ask_select`, `ask_confirm`

**Init / update (full):** above plus `bash`, `edit`, `write`

## Development

```sh
cargo build -p owly
cargo test -p owly
cargo clippy -p owly --all-targets -- -D warnings
```

### E2E CLI smoke tests

```sh
cargo build -p owly --release
./owly/scripts/e2e_cli.sh

# optional live LLM checks (uses ~/.owly credentials)
OWLY_E2E_LLM=1 OWLY_E2E_HOME="$HOME" ./owly/scripts/e2e_cli.sh
```

## Credits

- Original concept: [OpenWiki](https://github.com/langchain-ai/openwiki) (LangChain)
- Agent runtime: [elph-agent](https://github.com/riipandi/elph/tree/main/crates/elph-agent)
- LLM integration: [elph-ai](https://github.com/riipandi/elph/tree/main/crates/elph-ai)

## License

[Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). See [LICENSE-APACHE](../LICENSE-APACHE) and [NOTICE.md](../NOTICE.md).