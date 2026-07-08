---
title: "Configuration"
last_updated: 2026-07-08T10:57:29Z
category: operations
tags:
    - configuration
    - providers
    - setup
    - environment
status: published
---

# Configuration

## Overview

Owly uses a layered configuration approach: **CLI flags → Environment variables → Config file → Auto-detection**.

The configuration resolution order (from highest to lowest priority):

1. `--model` CLI flag (supports `provider/model` format like `"anthropic/claude-sonnet-5"`)
2. `OWLY_PROVIDER` and `OWLY_MODEL_ID` environment variables
3. `~/.owly/config.json` persistent config file
4. Auto-detection: provider chosen based on first available API key
5. Defaults: provider=`opencode`, model=`big-pickle`

**Source:** [`owly/src/config.rs`](../owly/src/config.rs) — `Config::resolve()` method.

---

## Environment Variables

| Variable        | Description       | Default      |
| --------------- | ----------------- | ------------ |
| `OWLY_PROVIDER` | LLM provider name | `opencode`   |
| `OWLY_MODEL_ID` | Model identifier  | `big-pickle` |

### Provider API Key Variables

Set the appropriate environment variable for your chosen provider:

| Provider              | Variable                         |
| --------------------- | -------------------------------- |
| OpenCode Zen          | `OPENCODE_API_KEY`               |
| OpenCode Go           | `OPENCODE_API_KEY`               |
| Anthropic             | `ANTHROPIC_API_KEY`              |
| OpenAI                | `OPENAI_API_KEY`                 |
| OpenRouter            | `OPENROUTER_API_KEY`             |
| Google                | `GOOGLE_API_KEY`                 |
| Google Vertex         | `GOOGLE_APPLICATION_CREDENTIALS` |
| DeepSeek              | `DEEPSEEK_API_KEY`               |
| xAI                   | `XAI_API_KEY`                    |
| Groq                  | `GROQ_API_KEY`                   |
| Fireworks             | `FIREWORKS_API_KEY`              |
| Together              | `TOGETHER_API_KEY`               |
| Mistral               | `MISTRAL_API_KEY`                |
| NVIDIA                | `NVIDIA_API_KEY`                 |
| Cerebras              | `CEREBRAS_API_KEY`               |
| Amazon Bedrock        | `AWS_ACCESS_KEY_ID`              |
| GitHub Copilot        | `GITHUB_TOKEN`                   |
| Cloudflare Workers AI | `CLOUDFLARE_API_TOKEN`           |
| Cloudflare AI Gateway | `CLOUDFLARE_API_TOKEN`           |
| Hugging Face          | `HF_TOKEN`                       |
| MoonshotAI            | `MOONSHOT_API_KEY`               |
| Z.AI                  | `ZAI_API_KEY`                    |
| Xiaomi                | `XIAOMI_API_KEY`                 |
| MiniMax               | `MINIMAX_API_KEY`                |
| Ant Ling              | `ANT_LING_API_KEY`               |

**Source:** [`owly/src/constants.rs`](../owly/src/constants.rs) — `provider_config()` function.

---

## Credential Storage

### `~/.owly/.env` File

Owly stores credentials and configuration in `~/.owly/.env`. This file is loaded automatically on startup.

Example:

```env
OWLY_PROVIDER=opencode
OWLY_MODEL_ID=big-pickle
OPENCODE_API_KEY=your-api-key-here
```

**Behavior:**

- Variables in `~/.owly/.env` are applied to the process environment **only if not already set** (process env takes precedence).
- Managed keys include all provider API keys plus `OWLY_PROVIDER` and `OWLY_MODEL_ID`.
- The file uses simple `KEY=VALUE` format with optional `"quoted"` values (newlines and special chars supported with escaping).

**Source:** [`owly/src/credentials.rs`](../owly/src/credentials.rs).

### `~/.owly/config.json` File

For persistent non-sensitive configuration:

```json
{
    "provider": "anthropic",
    "model_id": "claude-sonnet-5"
}
```

**Source:** [`owly/src/config.rs`](../owly/src/config.rs) — `ConfigFile` struct and `load_config_file()` / `save_config_file()` functions.

---

## Provider Auto-Detection

When no provider is explicitly configured, Owly auto-detects based on available API keys:

1. `OPENCODE_API_KEY` → `opencode`
2. `ANTHROPIC_API_KEY` → `anthropic`
3. `OPENAI_API_KEY` → `openai`
4. `OPENROUTER_API_KEY` → `openrouter`
5. `GOOGLE_API_KEY` → `google`
6. `DEEPSEEK_API_KEY` → `deepseek`
7. `GROQ_API_KEY` → `groq`
8. Falls back to `opencode` (default)

**Source:** [`owly/src/constants.rs`](../owly/src/constants.rs) — `resolve_configured_provider()`.

---

## Model Selection

Model can be specified in two formats:

1. **Simple model name:** `--model "claude-sonnet-5"` — uses the configured/auto-detected provider
2. **Provider/model format:** `--model "anthropic/claude-sonnet-5"` — overrides both provider and model

### Provider Default Models

| Provider   | Default Model             |
| ---------- | ------------------------- |
| OpenCode   | `big-pickle`              |
| Anthropic  | `claude-sonnet-5`         |
| OpenAI     | `gpt-5.4-mini`            |
| OpenRouter | `z-ai/glm-5.2`            |
| Google     | `gemini-2.5-flash`        |
| DeepSeek   | `deepseek-chat`           |
| Groq       | `llama-3.3-70b-versatile` |

---

## Security Notes

- **API keys are never written into documentation.** The diagnostics module redacts credentials from error messages and logs.
- **The `~/.owly/.env` file** contains sensitive credentials. Ensure appropriate file permissions.
- **Environment variables** take precedence over the `.env` file, so you can override per-session.
- **GitHub Copilot** auth uses `GITHUB_TOKEN` — be aware of token scope.

---

## Troubleshooting

### "No API key configured" error

Set the appropriate environment variable for your provider, or add it to `~/.owly/.env`.

### "Model not found" error

Verify the provider/model combination. Use `provider/model` format: `--model "opencode/big-pickle"`.

### Provider returns 500 errors

Try a different model with `--model`. Run with `OWLY_DEBUG=1` for provider metadata. See [`diagnostics.rs`](../owly/src/diagnostics.rs) for error handling.

### Tests

- [`config_test.rs`](../owly/tests/config_test.rs) — Config resolution and edge cases
- [`env_ext_test.rs`](../owly/tests/env_ext_test.rs) — Environment variable handling
- [`credentials_test.rs`](../owly/tests/credentials_test.rs) — `.env` file loading
