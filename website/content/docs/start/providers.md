# Providers

Elph talks to LLM providers through API keys and OAuth where the provider supports it. Catalogs ship with the binary and unpack to `CONFIG_DIR/providers/`. Local OpenAI-compatible endpoints usually need no cloud key.

## Supported catalogs

These provider ids have built-in model catalogs in `elph-ai`:

| Provider                           | Catalog id                                               | Auth                             |
| ---------------------------------- | -------------------------------------------------------- | -------------------------------- |
| Anthropic                          | `anthropic`                                              | `ANTHROPIC_API_KEY` or OAuth     |
| OpenAI                             | `openai`                                                 | `OPENAI_API_KEY`                 |
| OpenAI Codex                       | `openai-codex`                                           | ChatGPT subscription / Codex API |
| Azure OpenAI                       | `azure-openai-responses`                                 | Azure credentials                |
| Google Gemini                      | `google`                                                 | `GEMINI_API_KEY`                 |
| Google Vertex                      | `google-vertex`                                          | GCP credentials                  |
| xAI / Grok                         | `xai`                                                    | `XAI_API_KEY` or OAuth           |
| GitHub Copilot                     | `github-copilot`                                         | `COPILOT_GITHUB_TOKEN` / OAuth   |
| OpenRouter                         | `openrouter`                                             | `OPENROUTER_API_KEY`             |
| Groq                               | `groq`                                                   | `GROQ_API_KEY`                   |
| DeepSeek                           | `deepseek`                                               | `DEEPSEEK_API_KEY`               |
| Moonshot / Kimi                    | `moonshotai`                                             | `MOONSHOT_API_KEY`               |
| Kimi For Coding                    | `kimi-coding`                                            | provider key                     |
| OpenCode Zen                       | `opencode`                                               | `OPENCODE_API_KEY`               |
| OpenCode Go                        | `opencode-go`                                            | `OPENCODE_API_KEY`               |
| Amazon Bedrock                     | `amazon-bedrock`                                         | AWS credentials                  |
| Cerebras                           | `cerebras`                                               | API key                          |
| Fireworks                          | `fireworks`                                              | API key                          |
| Together                           | `together`                                               | API key                          |
| Mistral                            | `mistral`                                                | API key                          |
| MiniMax                            | `minimax`                                                | API key                          |
| Hugging Face                       | `huggingface`                                            | API key                          |
| NVIDIA                             | `nvidia`                                                 | API key                          |
| Hyper                              | `hyper`                                                  | API key                          |
| Kilo                               | `kilo`                                                   | API key                          |
| Nara                               | `nara-router`                                            | API key                          |
| Z.AI                               | `zai`                                                    | API key                          |
| Qwen token plans                   | `qwen-token-plan`                                        | API key                          |
| Cloudflare Workers AI / AI Gateway | `cloudflare-workers-ai`, `cloudflare-ai-gateway`         | Cloudflare                       |
| Vercel AI Gateway                  | `vercel-ai-gateway`                                      | API key                          |
| Ollama Cloud                       | `ollama-cloud`                                           | optional                         |
| Local OpenAI-compatible            | disk overlay (`ollama`, `lmstudio`, `vllm`, `localhost`) | optional                         |

Image generation currently goes through OpenRouter (`openrouter-images`). Additional catalogs (Agnes, Ant Ling, Baseten, Infron, Neuralwatt, OpenGateway, Sumopod, TokenRouter, Wafer, Xiaomi, …) ship the same way — `elph provider` lists what is on disk after first launch.

## Environment variables

| Variable               | Provider                      |
| ---------------------- | ----------------------------- |
| `OPENAI_API_KEY`       | OpenAI                        |
| `ANTHROPIC_API_KEY`    | Anthropic (also OAuth)        |
| `XAI_API_KEY`          | xAI / Grok                    |
| `OPENCODE_API_KEY`     | OpenCode Zen / Go             |
| `COPILOT_GITHUB_TOKEN` | GitHub Copilot                |
| `GITHUB_TOKEN`         | Fallback for Copilot exchange |
| `DEEPSEEK_API_KEY`     | DeepSeek                      |
| `MOONSHOT_API_KEY`     | Moonshot / Kimi               |
| `OPENROUTER_API_KEY`   | OpenRouter                    |
| `GROQ_API_KEY`         | Groq                          |
| `GEMINI_API_KEY`       | Google                        |

Provider JSON may also reference keys as `env.VAR`, `$VAR`, `${VAR}`, or shell expansions.

Local providers (`localhost`, `ollama`, `lmstudio`, `vllm`) treat auth as optional.

## Interactive login

In the TUI:

```text
/provider connect          # OAuth or API key
/mcp auth <server>         # MCP OAuth
```

Credentials persist in `CONFIG_DIR/auth.json` (mode `0600` on Unix). Prefer environment variables for CI.

```sh
elph provider          # list / manage providers
elph doctor            # show discovered config
elph models            # list models after auth resolves
```

After `/provider connect`, credentials are written to `auth.json` and injected into the live session. You should not need to restart Elph.

See [Custom models](/docs/customize/models) to add or override catalogs.
