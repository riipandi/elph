# Owly

Owly is a CLI that writes and maintains documentation for your codebases, built specifically for
agents using [elph-agent](https://github.com/riipandi/elph/tree/main/crates/elph-agent) and
[elph-ai](https://github.com/riipandi/elph/tree/main/crates/elph-ai).

## Install

Install from crates.io:

```sh
cargo install --locked owly
```

Or build from source:

```sh
cargo install --path owly
```

Or run directly without installing:

```sh
cargo run -p owly -- [OPTIONS] [MESSAGE]
```

## Quick Start

Initialize Owly, configure your model and API key, then generate documentation:

```sh
owly --init
```

Then to ensure your documentation stays up-to-date, run:

```sh
owly --update
```

## Usage

Start Owly with an initial request:

```sh
owly "Please generate documentation for this repository"
```

Run a single command and exit:

```sh
owly -p "Summarize what you can do"
```

Initialize Owly:

```sh
owly --init
```

Update existing documentation:

```sh
owly --update
```

Show help:

```sh
owly --help
```

`owly` creates initial documentation in `openwiki/` when no wiki exists. If `openwiki/` already exists, it refreshes that documentation from repository changes. Use `-p` or `--print` for a one-shot non-interactive run that prints the final assistant output.

Use `-v` or `--verbose` to show streaming response and thinking from the LLM:

```sh
owly -v "Read the Cargo.toml file"
```

## Customizing

Owly supports OpenCode (default), OpenRouter, Anthropic, OpenAI, Google, DeepSeek, Groq, Fireworks, Together, Mistral, and more out of the box. By default, the model is `opencode/big-pickle`.

### Model Selection

Use `--model` to specify a model:

```sh
owly --model "anthropic/claude-sonnet-5" "What is this project?"
owly --model "openai/gpt-5.4-mini" "Explain the architecture"
```

### Environment Variables

| Variable        | Description     | Default      |
| --------------- | --------------- | ------------ |
| `OWLY_PROVIDER` | Provider to use | `opencode`   |
| `OWLY_MODEL_ID` | Model ID to use | `big-pickle` |

### Provider API Keys

| Provider   | Environment Variable |
| ---------- | -------------------- |
| OpenCode   | `OPENCODE_API_KEY`   |
| Anthropic  | `ANTHROPIC_API_KEY`  |
| OpenAI     | `OPENAI_API_KEY`     |
| OpenRouter | `OPENROUTER_API_KEY` |
| Google     | `GOOGLE_API_KEY`     |
| DeepSeek   | `DEEPSEEK_API_KEY`   |
| Groq       | `GROQ_API_KEY`       |
| Fireworks  | `FIREWORKS_API_KEY`  |
| Together   | `TOGETHER_API_KEY`   |
| Mistral    | `MISTRAL_API_KEY`    |

### Configuration File

Owly stores configuration in `~/.owly/.env`. Example:

```env
OWLY_PROVIDER=opencode
OWLY_MODEL_ID=big-pickle
OPENCODE_API_KEY=your-api-key-here
```

## Documentation Structure

Owly creates documentation in the `openwiki/` directory with the following structure:

```
openwiki/
├── quickstart.md           # Entry point - overview and navigation
├── .last-update.json       # Update metadata
├── architecture/           # Architecture documentation
├── workflows/              # Workflow documentation
├── domain/                 # Domain-specific documentation
├── api/                    # API documentation
├── operations/             # Operations documentation
├── integrations/           # Integration documentation
└── testing/                # Testing documentation
```

### Frontmatter

Every documentation file includes YAML frontmatter:

```yaml
---
title: "Quickstart Guide"
last_updated: 2024-01-15T10:30:00Z
category: quickstart
tags:
    - getting-started
    - overview
status: published
---
```

## Tools

Owly uses elph-agent tools for filesystem operations:

### Read-only Tools (Chat Mode)

- `read` - Read file contents
- `grep` - Search text across files
- `find` - Find files matching a pattern
- `ls` - List directory contents

### Full Tools (Init/Update Mode)

- `read` - Read file contents
- `bash` - Run shell commands
- `edit` - Edit files (exact string replacement)
- `write` - Write file contents
- `grep` - Search text across files
- `find` - Find files matching a pattern
- `ls` - List directory contents

## Development

### Build

```sh
cargo build -p owly
```

### Test

```sh
cargo test -p owly
```

### Lint

```sh
cargo clippy -p owly --all-targets -- -D warnings
```

## Credits

- Original concept: [OpenWiki](https://github.com/langchain-ai/openwiki) by LangChain
- Agent runtime: [elph-agent](https://github.com/riipandi/elph/tree/main/crates/elph-agent)
- LLM integration: [elph-ai](https://github.com/riipandi/elph/tree/main/crates/elph-ai)

## License

Licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
See [LICENSE-APACHE](../LICENSE-APACHE) and [NOTICE.md](../NOTICE.md).
