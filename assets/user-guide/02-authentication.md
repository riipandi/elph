# Authentication

Elph authenticates to LLM providers via API keys (and OAuth where supported).

## Environment variables

Common keys (provider-dependent):

| Variable            | Provider family   |
| ------------------- | ----------------- |
| `OPENAI_API_KEY`    | OpenAI-compatible |
| `ANTHROPIC_API_KEY` | Anthropic         |
| `OPENCODE_API_KEY`  | OpenCode Zen / Go |
| `DEEPSEEK_API_KEY`  | DeepSeek          |
| `MOONSHOT_API_KEY`  | Moonshot / Kimi   |

Provider JSON under `CONFIG_DIR/providers/*.json` may also reference keys as `env.VAR`,
`$VAR`, `${VAR}`, or shell expansions.

## Credential store

Credentials may be persisted in `CONFIG_DIR/auth.json` (mode `0600` on Unix). Prefer env
vars for CI and shared machines.

## CLI

```sh
elph provider          # list / manage providers
elph doctor            # show discovered config
elph models            # list models after auth resolves
```

See also [Custom models](08-custom-models.md).
