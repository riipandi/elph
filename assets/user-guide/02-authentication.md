# Authentication

Elph authenticates to LLM providers via API keys and OAuth (where supported). Local
OpenAI-compatible endpoints typically need **no** cloud API key.

## Environment variables

Common keys (provider-dependent):

| Variable               | Provider family                          |
| ---------------------- | ---------------------------------------- |
| `OPENAI_API_KEY`       | OpenAI                                   |
| `ANTHROPIC_API_KEY`    | Anthropic (also OAuth)                   |
| `XAI_API_KEY`          | xAI / Grok (also OAuth subscription)     |
| `OPENCODE_API_KEY`     | OpenCode Zen / Go                        |
| `COPILOT_GITHUB_TOKEN` | GitHub Copilot (session token or GitHub OAuth/PAT — exchanged automatically) |
| `GITHUB_TOKEN`         | Fallback for Copilot exchange            |
| `DEEPSEEK_API_KEY`     | DeepSeek                                 |
| `MOONSHOT_API_KEY`     | Moonshot / Kimi                          |

Provider JSON under `CONFIG_DIR/providers/*.json` may also reference keys as `env.VAR`,
`$VAR`, `${VAR}`, or shell expansions.

### Local / no-auth providers

Disk-only providers whose `baseUrl` is loopback/local (`localhost`, `127.0.0.1`, …) or
whose id contains `local`, `ollama`, `lmstudio`, or `vllm` resolve auth as **optional**:
missing env keys no longer fail with `No API key for provider: local-llm`. Requests send
an empty bearer (accepted by most local OpenAI-compatible servers).

## Credential store

Credentials may be persisted in `CONFIG_DIR/auth.json` (mode `0600` on Unix; sealed
envelope). Prefer env vars for CI and shared machines.

Interactive login:

```text
/provider connect          # OAuth or API key
/mcp auth <server>         # MCP OAuth (http/sse)
```

## GitHub Copilot

The Copilot chat API expects a **session** token (`tid=…;exp=…;proxy-ep=…;…`), not a bare
GitHub PAT. Elph exchanges GitHub OAuth/PAT tokens via
`GET …/copilot_internal/v2/token` when the stored/env value is not already a session token.
If you see `invalid token: missing = param`, re-run `/provider connect` for GitHub Copilot
or set `COPILOT_GITHUB_TOKEN` to a valid GitHub token so it can be exchanged.

After `/provider connect` (OAuth or API key), credentials are written to `auth.json` **and**
injected into the live session models store — you should not need to restart Elph.

### Plan-gated models (Free / Student / paid)

Copilot models depend on your **subscription**. [Copilot Free and Student](https://docs.github.com/en/copilot/concepts/models/auto-model-selection)
only get models through **Auto model selection** — picking a premium id (e.g. Claude Opus)
returns `Invalid request (400): The requested model is not supported`.

After login (and on session start for older credentials), Elph:

1. Fetches plan-available model ids from the Copilot `GET /models` API.
2. Filters the live `github-copilot` catalog to those ids.
3. Falls back to `auto`, then `gpt-5-mini`, then the first remaining model when the
   configured default is not on your plan.

Use model id `auto` (display name **Auto**) for Free/Student and whenever you want
server-side routing. Paid plans can still select individual models that appear in the
filtered list.

Device login flow:

1. Prompt: GitHub host (press **Enter** for `github.com`, or type an enterprise domain).
2. Open the verification URL and enter the user code.
3. Elph polls GitHub, exchanges for a Copilot session token, fetches available models, then finishes.

Skip the host prompt with `ELPH_GITHUB_HOST=github.com` (or your enterprise domain).

## xAI OAuth

xAI supports device-code OAuth (Grok subscription) in addition to `XAI_API_KEY`. Token
expiry is stored in epoch milliseconds; after upgrading Elph, re-connect if an old
credential was saved with a broken expiry scale.

## MCP OAuth

Remote MCP servers (`http` / `sse`) use browser PKCE. Credentials are sealed under the
same `auth.json`. Conflict policy (`authConflict` in mcp.json): `error` (default),
`preferEnv`, or `preferOauth`. See the MCP user guide for details.

## CLI

```sh
elph provider          # list / manage providers
elph doctor            # show discovered config
elph models            # list models after auth resolves
elph mcp auth <name>   # MCP OAuth login
```

See also [Custom models](08-custom-models.md).
