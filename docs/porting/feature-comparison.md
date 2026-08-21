# Feature Comparison: pi (TypeScript) vs Elph (Rust)

**Snapshot:** pi `cced6a21` (v0.82.1 + Unreleased) · Elph `7ac3955` (Sprint 6 + Sprint 7)

Mapping: `@earendil-works/pi-ai` → `crates/elph-ai`, `@earendil-works/pi-agent-core` → `crates/elph-agent`.

---

## elph-ai / pi-ai

### Architecture & Core

| Feature                                             | pi-ai                                | elph-ai                                      | Status       |
| --------------------------------------------------- | ------------------------------------ | -------------------------------------------- | ------------ |
| Provider streaming (`Models`, `stream`, `complete`) | `models.ts`                          | `src/models/collection.rs`                   | **[Parity]** |
| Model catalog (built-in providers)                  | `models.generated.ts` + `providers/` | `models/*.json` + `src/providers/builtin.rs` | **[Parity]** |
| Provider registration                               | `createModels()`, provider factories | `Provider` struct + `define_catalog!`        | **[Parity]** |
| Image models                                        | `images-models.ts`                   | `src/images/`                                | **[Parity]** |
| Lazy provider loading                               | `api/lazy.ts`                        | `src/api/faux.rs`                            | **[Parity]** |
| `Transport` (SSE, WebSocket, auto)                  | `types.ts`                           | `src/types/mod.rs`                           | **[Parity]** |
| `SessionAffinityFormat`                             | `types.ts`                           | `src/types/mod.rs`                           | **[Parity]** |
| Per-request `fetch` injection                       | `Unreleased`                         | `StreamOptions.client`                       | **[Parity]** |
| `SimpleStreamOptions`                               | `simple-options.ts`                  | `src/api/simple_options.rs`                  | **[Parity]** |

### API Adapters

| Feature                 | pi-ai                            | elph-ai                              | Status       |
| ----------------------- | -------------------------------- | ------------------------------------ | ------------ |
| Anthropic Messages API  | `api/anthropic-messages.ts`      | `src/api/anthropic_messages.rs`      | **[Parity]** |
| OpenAI Responses API    | `api/openai-responses.ts`        | `src/api/openai_responses.rs`        | **[Parity]** |
| OpenAI Completions API  | `api/openai-completions.ts`      | `src/api/openai_completions.rs`      | **[Parity]** |
| OpenAI Codex Responses  | `api/openai-codex-responses.ts`  | `src/api/openai_codex_responses.rs`  | **[Parity]** |
| Azure OpenAI Responses  | `api/azure-openai-responses.ts`  | `src/api/azure_openai_responses.rs`  | **[Parity]** |
| Amazon Bedrock Converse | `api/bedrock-converse-stream.ts` | `src/api/bedrock_converse_stream.rs` | **[Parity]** |
| Google Gemini           | `api/google-generative-ai.ts`    | `src/api/google_generative_ai.rs`    | **[Parity]** |
| Google Vertex AI        | `api/google-vertex.ts`           | `src/api/google_vertex.rs`           | **[Parity]** |
| Mistral Conversations   | `api/mistral-conversations.ts`   | `src/api/mistral_conversations.rs`   | **[Parity]** |
| Cloudflare              | `api/cloudflare.ts`              | `src/api/cloudflare.rs`              | **[Parity]** |
| OpenRouter Images       | `api/openrouter-images.ts`       | `src/api/openrouter_images.rs`       | **[Parity]** |
| pi-messages gateway     | `api/pi-messages.ts`             | `src/api/pi_messages.rs`             | **[Parity]** |
| GitHub Copilot headers  | `api/github-copilot-headers.ts`  | `src/api/github_copilot_headers.rs`  | **[Parity]** |
| OpenAI prompt cache     | `api/openai-prompt-cache.ts`     | `src/api/openai_prompt_cache.rs`     | **[Parity]** |
| Constrained sampling    | `api/constrained-sampling.ts`    | `src/types/mod.rs`                   | **[Parity]** |
| Transform messages      | `api/transform-messages.ts`      | `src/api/transform_messages.rs`      | **[Parity]** |

### Auth

| Feature                                   | pi-ai                                | elph-ai                                          | Status           |
| ----------------------------------------- | ------------------------------------ | ------------------------------------------------ | ---------------- |
| `AuthContext` / `DefaultAuthContext`      | `auth/context.ts`                    | `src/auth/context.rs`                            | **[Parity]**     |
| `CredentialStore`                         | `auth/credential-store.ts`           | `src/auth/credential_store.rs`                   | **[Parity]**     |
| `CredentialStore.list()`                  | v0.81.0                              | `src/auth/credential_store.rs`                   | **[Parity]**     |
| Auth types (ApiKey, OAuth, Bearer)        | `auth/types.ts`                      | `src/auth/types.rs`                              | **[Parity]**     |
| Auth resolution (`resolve_provider_auth`) | `auth/resolve.ts`                    | `src/auth/resolve.rs`                            | **[Parity]**     |
| `ModelsError` with cause chain            | v0.82.1                              | `src/auth/resolve.rs`                            | **[Parity]**     |
| `ModelsStore` + `etag`                    | `models-store.ts`                    | `src/auth/models_store.rs`                       | **[Parity]**     |
| Anthropic OAuth (PKCE)                    | `auth/oauth/anthropic-oauth.ts`      | `src/auth/oauth/anthropic.rs`                    | **[Parity]**     |
| OpenAI Codex OAuth                        | `auth/oauth/openai-codex-oauth.ts`   | `src/auth/oauth/openai_codex.rs`                 | **[Parity]**     |
| GitHub Copilot OAuth                      | `auth/oauth/github-copilot-oauth.ts` | `src/auth/oauth/` (via `github_copilot_oauth()`) | **[Parity]**     |
| Hyper OAuth (Elph-only)                   | —                                    | `src/auth/oauth/hyper.rs`                        | **[Elph delta]** |
| Kimi Code OAuth                           | v0.82.0                              | `src/auth/oauth/kimi.rs`                         | **[Parity]**     |
| OpenRouter OAuth PKCE                     | v0.82.0                              | `src/auth/oauth/openrouter.rs`                   | **[Parity]**     |
| Radius OAuth gateway                      | `auth/oauth/radius.ts`               | `src/auth/oauth/radius.rs`                       | **[Parity]**     |
| `env_api_key_auth`                        | `env-api-keys.ts`                    | `src/auth/helpers.rs`                            | **[Parity]**     |
| Bun OAuth server                          | `bun-oauth.ts`                       | `src/auth/oauth/` (PKCE module)                  | **[Parity]**     |

### Types

| Feature                                              | pi-ai        | elph-ai                                | Status       |
| ---------------------------------------------------- | ------------ | -------------------------------------- | ------------ |
| `Model`, `Message`, `Tool`, `ToolCall`               | `types.ts`   | `src/types/mod.rs`                     | **[Parity]** |
| `ThinkingLevel` (Minimal..Max)                       | `types.ts`   | `src/types/mod.rs`                     | **[Parity]** |
| `CacheRetention` (None, Short, Long)                 | `types.ts`   | `src/types/mod.rs`                     | **[Parity]** |
| `StopReason` (Stop, Length, ToolUse, Error, Aborted) | `types.ts`   | `src/types/mod.rs`                     | **[Parity]** |
| `pending_stop_reason` mid-stream                     | `Unreleased` | `AssistantMessage.pending_stop_reason` | **[Parity]** |
| `Usage` metadata on messages                         | v0.81.0      | `src/types/mod.rs`                     | **[Parity]** |
| `Tool.constrained_sampling`                          | v0.82.0      | `src/types/mod.rs`                     | **[Parity]** |
| `ConstrainedSamplingConfig`                          | v0.82.0      | `src/types/mod.rs`                     | **[Parity]** |
| Compat flags (`supports_*`)                          | v0.82.0      | `src/types/mod.rs`                     | **[Parity]** |
| `ModelCost.tiers`                                    | v0.80.6      | `src/types/mod.rs`                     | **[Parity]** |
| `AssistantMessageDiagnostic`                         | types.ts     | `src/types/mod.rs`                     | **[Parity]** |

### Provider Implementations

| Feature                                           | pi-ai                                 | elph-ai                                          | Status           |
| ------------------------------------------------- | ------------------------------------- | ------------------------------------------------ | ---------------- |
| Anthropic                                         | `providers/anthropic.ts`              | `src/providers/builtin.rs`                       | **[Parity]**     |
| OpenAI                                            | `providers/openai.ts`                 | `src/providers/builtin.rs`                       | **[Parity]**     |
| OpenAI Codex                                      | `providers/openai-codex.ts`           | `src/providers/builtin.rs`                       | **[Parity]**     |
| Amazon Bedrock                                    | `providers/amazon-bedrock.ts`         | `src/providers/builtin.rs`                       | **[Parity]**     |
| Google / Google Vertex                            | `providers/google.ts`                 | `src/providers/builtin.rs`                       | **[Parity]**     |
| Mistral                                           | `providers/mistral.ts`                | `src/providers/builtin.rs`                       | **[Parity]**     |
| GitHub Copilot                                    | `providers/github-copilot.ts`         | `src/providers/builtin.rs`                       | **[Parity]**     |
| Kimi Coding                                       | `providers/kimi-coding.ts`            | `src/providers/builtin.rs`                       | **[Parity]**     |
| Qwen Token Plan                                   | v0.81.0                               | `src/providers/builtin.rs`                       | **[Parity]**     |
| OpenRouter                                        | `providers/openrouter.ts`             | `src/providers/builtin.rs`                       | **[Parity]**     |
| Groq, Fireworks, Together, xAI, DeepSeek          | `providers/*.ts`                      | `src/providers/builtin.rs`                       | **[Parity]**     |
| Azure OpenAI Responses                            | `providers/azure-openai-responses.ts` | `src/providers/builtin.rs`                       | **[Parity]**     |
| Cloudflare (AI Gateway, Workers AI)               | `providers/cloudflare-*.ts`           | `src/providers/builtin.rs`                       | **[Parity]**     |
| Cerebras, HuggingFace, NVIDIA, MinMax, MoonshotAI | `providers/*.ts`                      | `src/providers/builtin.rs`                       | **[Parity]**     |
| OpenCode, OpenCode Go                             | `providers/opencode*.ts`              | `src/providers/builtin.rs`                       | **[Parity]**     |
| Vercel AI Gateway                                 | `providers/vercel-ai-gateway.ts`      | `src/providers/builtin.rs`                       | **[Parity]**     |
| Z.AI, Z.AI Coding CN                              | `providers/zai*.ts`                   | `src/providers/builtin.rs`                       | **[Parity]**     |
| Xiaomi Token Plan                                 | `providers/xiaomi*.ts`                | `src/providers/builtin.rs`                       | **[Parity]**     |
| Hyper (Elph-only)                                 | —                                     | `src/providers/builtin.rs` + `models/hyper.json` | **[Elph delta]** |
| Radius                                            | `providers/radius.ts`                 | —                                                | **[Gap P2]**     |
| Faux (test) provider                              | `providers/faux.ts`                   | `src/providers/faux.rs`                          | **[Parity]**     |

### Utilities

| Feature                | pi-ai                       | elph-ai                         | Status                      |
| ---------------------- | --------------------------- | ------------------------------- | --------------------------- |
| Retry patterns         | `utils/retry.ts`            | `src/utils/retry.rs`            | **[Parity]**                |
| `is_transient_error()` | `utils/provider-retry.ts`   | `src/utils/retry.rs`            | **[Parity]**                |
| `retryAssistantCall()` | v0.82.0                     | `src/utils/retry.rs`            | **[Parity]**                |
| Deferred tools         | `utils/deferred-tools.ts`   | `src/utils/deferred_tools.rs`   | **[Parity]**                |
| Diagnostics            | `utils/diagnostics.ts`      | `src/utils/diagnostics.rs`      | **[Parity]**                |
| Context estimate       | `utils/estimate.ts`         | `src/utils/estimate.rs`         | **[Parity]**                |
| Event stream           | `utils/event-stream.ts`     | `src/utils/event_stream.rs`     | **[Parity]**                |
| `contentText`          | `utils/text.ts`             | `src/utils/text.rs`             | **[Parity]**                |
| TypeBox helpers        | `utils/typebox-helpers.ts`  | —                               | **[N/A]** (Rust uses serde) |
| `uuidv7`               | `utils/uuid.ts`             | — (uses `ulid`)                 | **[N/A]**                   |
| JSON parse             | `utils/json-parse.ts`       | `src/utils/json_parse.rs`       | **[Parity]**                |
| Overflow handling      | `utils/overflow.ts`         | `src/utils/overflow.rs`         | **[Parity]**                |
| Validation             | `utils/validation.ts`       | `src/utils/validation.rs`       | **[Parity]**                |
| Sanitize unicode       | `utils/sanitize-unicode.ts` | `src/utils/sanitize_unicode.rs` | **[Parity]**                |
| Headers                | `utils/headers.ts`          | `src/utils/headers.rs`          | **[Parity]**                |
| Provider env           | `utils/provider-env.ts`     | `src/utils/provider_env.rs`     | **[Parity]**                |
| Hash                   | `utils/hash.ts`             | `src/utils/hash.rs`             | **[Parity]**                |
| Error body             | `utils/error-body.ts`       | `src/utils/error_body.rs`       | **[Parity]**                |
| Session resources      | `session-resources.ts`      | `src/session_resources.rs`      | **[Parity]**                |
| Abort signals          | `utils/abort-signals.ts`    | `CancellationToken`             | **[Parity]**                |
| Node HTTP proxy        | `utils/node-http-proxy.ts`  | `src/api/http_proxy.rs`         | **[Parity]**                |

### Model Catalogs

| Feature                           | pi-ai                                 | elph-ai                                                | Status           |
| --------------------------------- | ------------------------------------- | ------------------------------------------------------ | ---------------- |
| Anthropic                         | `providers/anthropic.models.ts`       | `models/anthropic.json`                                | **[Parity]**     |
| Amazon Bedrock                    | `providers/amazon-bedrock.models.ts`  | `models/amazon_bedrock.json`                           | **[Parity]**     |
| OpenAI                            | `providers/openai.models.ts`          | `models/openai.json`                                   | **[Parity]**     |
| OpenAI Codex                      | `providers/openai-codex.models.ts`    | `models/openai_codex.json`                             | **[Parity]**     |
| GitHub Copilot                    | `providers/github-copilot.models.ts`  | `models/github_copilot.json`                           | **[Parity]**     |
| Kimi Coding                       | `providers/kimi-coding.models.ts`     | `models/kimi_coding.json`                              | **[Parity]**     |
| Qwen Token Plan                   | `providers/qwen-token-plan.models.ts` | `models/qwen_token_plan.json`                          | **[Parity]**     |
| Claude Opus 5                     | v0.82.1                               | `models/anthropic.json` + `models/amazon_bedrock.json` | **[Parity]**     |
| Claude Opus 5 (Copilot `minimal`) | `Unreleased`                          | `models/github_copilot.json`                           | **[Parity]**     |
| Hyper (Elph-only)                 | —                                     | `models/hyper.json`                                    | **[Elph delta]** |

---

## elph-agent / pi-agent-core

### Agent Loop

| Feature                                  | pi-agent-core   | elph-agent                               | Status           |
| ---------------------------------------- | --------------- | ---------------------------------------- | ---------------- |
| Agent loop (`runAgentLoop`)              | `agent-loop.ts` | `src/runtime/run_loop.rs`                | **[Parity]**     |
| `Agent` class                            | `agent.ts`      | `src/agent/mod.rs`                       | **[Parity]**     |
| `AgentContext`                           | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `AgentLoopConfig`                        | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `StreamFn`                               | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `ConvertToLlmFn`                         | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `TransformContextFn`                     | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `BeforeToolCallFn` / `AfterToolCallFn`   | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `PrepareNextTurnFn`                      | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `GetQueuedMessagesFn`                    | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `shouldStopAfterTurn`                    | `agent-loop.ts` | `AgentLoopConfig.should_stop_after_turn` | **[Parity]**     |
| Tool execution (sequential/parallel)     | `agent-loop.ts` | `src/runtime/exec/dispatch.rs`           | **[Parity]**     |
| Tool call preparation + validation       | `agent-loop.ts` | `src/runtime/exec/prepare.rs`            | **[Parity]**     |
| `fail_tool_calls_from_truncated_message` | —               | `src/runtime/exec/mod.rs`                | **[Parity]**     |
| `AgentEvent` stream                      | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| `AgentState`                             | `types.ts`      | `src/runtime/loop_config.rs`             | **[Parity]**     |
| Prompt encoding (TOON)                   | —               | `src/prompt/encoding/`                   | **[Elph delta]** |
| `AgentThinkingLevel`                     | `types.ts`      | `src/types/enums.rs`                     | **[Parity]**     |
| `ToolExecutionMode`                      | `types.ts`      | `src/types/enums.rs`                     | **[Parity]**     |
| `QueueMode`                              | `types.ts`      | `src/types/enums.rs`                     | **[Parity]**     |

### Harness

| Feature                           | pi-agent-core                 | elph-agent                            | Status       |
| --------------------------------- | ----------------------------- | ------------------------------------- | ------------ |
| `AgentHarness`                    | `harness/agent-harness.ts`    | `src/agent/harness/mod.rs`            | **[Parity]** |
| `AgentHarnessOptions`             | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |
| `AgentHarnessStreamOptions`       | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |
| `AgentHarnessResources`           | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |
| `SystemPrompt` / `SystemPromptFn` | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |
| Skills                            | `harness/skills.ts`           | `src/agent/harness/types/options.rs`  | **[Parity]** |
| Prompt templates                  | `harness/prompt-templates.ts` | `src/agent/harness/types/options.rs`  | **[Parity]** |
| Hook registry (`HookRegistry`)    | `harness/types.ts`            | `src/agent/harness/hooks.rs`          | **[Parity]** |
| `AgentHarnessEvent`               | `harness/types.ts`            | `src/agent/harness/hooks.rs`          | **[Parity]** |
| `AgentHarnessOwnEvent`            | —                             | `src/agent/harness/types/events.rs`   | **[Parity]** |
| Compaction ops                    | `harness/compaction/`         | `src/agent/harness/compaction_ops.rs` | **[Parity]** |
| Compaction retry lifecycle        | v0.81.1                       | `compact_with_retry()`                | **[Parity]** |
| Prompt ops                        | `harness/`                    | `src/agent/harness/prompt_ops.rs`     | **[Parity]** |
| Plan mode                         | `harness/`                    | `src/agent/harness/plan_mode.rs`      | **[Parity]** |
| Tree navigation                   | `harness/`                    | `src/agent/harness/tree_nav.rs`       | **[Parity]** |
| `CompactionSettings`              | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |
| `DEFAULT_COMPACTION_SETTINGS`     | `harness/types.ts`            | `src/agent/harness/types/options.rs`  | **[Parity]** |

### Session Storage

| Feature                            | pi-agent-core                       | elph-agent                                    | Status           |
| ---------------------------------- | ----------------------------------- | --------------------------------------------- | ---------------- |
| `SessionStorage` trait             | `harness/session/session.ts`        | `src/session/types.rs`                        | **[Parity]**     |
| `InMemorySessionStorage`           | `harness/session/memory-storage.ts` | `src/session/backends/memory.rs`              | **[Parity]**     |
| `SessionDirStorage`                | —                                   | `src/session/backends/session_dir/storage.rs` | **[Elph delta]** |
| `TursoSessionStorage`              | —                                   | `src/session/backends/turso.rs`               | **[Elph delta]** |
| `JsonlSessionStorage`              | `harness/session/jsonl-storage.ts`  | `src/session/backends/jsonl.rs`               | **[Parity]**     |
| `SessionMetadata`                  | `harness/session/session.ts`        | `src/session/types.rs`                        | **[Parity]**     |
| `SessionTreeEntry`                 | `harness/session/session.ts`        | `src/session/types.rs`                        | **[Parity]**     |
| `SessionIndex`                     | `harness/session/session.ts`        | `src/session/types.rs`                        | **[Parity]**     |
| `get_path_to_root()`               | `harness/session/session.ts`        | `src/session/types.rs`                        | **[Parity]**     |
| `get_path_to_root_or_compaction()` | v0.81.0                             | `src/session/storage_utils.rs`                | **[Parity]**     |
| `get_entries_cursor()`             | v0.81.0                             | `src/session/storage_utils.rs`                | **[Parity]**     |
| `get_statistics()`                 | v0.81.0                             | `src/session/storage_utils.rs`                | **[Parity]**     |
| `CheckpointTail`                   | v0.81.0                             | `src/session/types.rs`                        | **[Parity]**     |
| `Session` wrapper                  | `harness/session/session.ts`        | `src/session/tree.rs`                         | **[Parity]**     |
| `SessionRepo`                      | `harness/session/memory-repo.ts`    | `src/session/repo.rs`                         | **[Parity]**     |
| `RepoUtils`                        | `harness/session/repo-utils.ts`     | `src/session/repo_utils.rs`                   | **[Parity]**     |
| Session context builder            | —                                   | `src/session/context.rs`                      | **[Parity]**     |
| `SessionContextBuildOptions`       | —                                   | `src/session/context.rs`                      | **[Parity]**     |
| Entry transforms / projectors      | v0.80.4                             | `src/session/context.rs`                      | **[Parity]**     |

### Tool System

| Feature                                               | pi-agent-core                          | elph-agent                                | Status           |
| ----------------------------------------------------- | -------------------------------------- | ----------------------------------------- | ---------------- |
| `AgentTool`                                           | `harness/tools/index.ts`               | `src/tools/types.rs`                      | **[Parity]**     |
| `AgentToolResult`                                     | `harness/tools/index.ts`               | `src/tools/types.rs`                      | **[Parity]**     |
| `ToolExecuteFn`                                       | `harness/tools/index.ts`               | `src/tools/types.rs`                      | **[Parity]**     |
| `ToolContext` (context-aware tools)                   | v0.82.0                                | `src/tools/types.rs`                      | **[Parity]**     |
| `AgentHarnessTool` trait                              | v0.82.0                                | `src/tools/types.rs`                      | **[Parity]**     |
| `context_aware_tool()` helper                         | v0.82.0                                | `src/tools/types.rs`                      | **[Parity]**     |
| `read` tool                                           | `harness/tools/read.ts`                | `src/tools/read_file.rs`                  | **[Parity]**     |
| `write` tool                                          | `harness/tools/write.ts`               | `src/tools/write_file.rs`                 | **[Parity]**     |
| `edit` / `edit-diff` tool                             | `harness/tools/edit.ts`                | `src/tools/edit_file.rs`                  | **[Parity]**     |
| `bash` tool                                           | `harness/tools/bash.ts`                | `src/tools/shell_exec.rs`                 | **[Parity]**     |
| `grep` tool                                           | —                                      | `src/tools/grep.rs`                       | **[Elph delta]** |
| `find_path` tool                                      | —                                      | `src/tools/find_path.rs`                  | **[Elph delta]** |
| `list_dir` tool                                       | —                                      | `src/tools/list_dir.rs`                   | **[Elph delta]** |
| Web fetch / search tools                              | —                                      | `src/tools/web/`                          | **[Elph delta]** |
| `copy_path`, `create_dir`, `delete_path`, `move_path` | —                                      | `src/tools/*.rs`                          | **[Elph delta]** |
| MCP client                                            | —                                      | `src/tools/mcp/`                          | **[Elph delta]** |
| Collaboration tools                                   | —                                      | `src/tools/collaboration.rs`              | **[Elph delta]** |
| `fff_picker` (grep output)                            | —                                      | `src/tools/fff_picker.rs`                 | **[Elph delta]** |
| Path utils                                            | `harness/tools/path-utils.ts`          | `src/tools/common.rs`                     | **[Parity]**     |
| File mutation queue                                   | `harness/tools/file-mutation-queue.ts` | `src/tools/file_mutation_queue.rs`        | **[Parity]**     |
| Image tool                                            | `harness/tools/image.ts`               | `src/tools/image.rs`                      | **[Parity]**     |
| Shell output capture                                  | `harness/utils/shell-output.ts`        | `src/agent/harness/utils/shell_output.rs` | **[Parity]**     |
| Truncation                                            | `harness/utils/truncate.ts`            | `src/agent/harness/utils/truncate.rs`     | **[Parity]**     |

### Compaction

| Feature                          | pi-agent-core         | elph-agent                               | Status       |
| -------------------------------- | --------------------- | ---------------------------------------- | ------------ |
| `compact()`                      | `harness/compaction/` | `src/compaction/compact.rs`              | **[Parity]** |
| `prepare_compaction()`           | `harness/compaction/` | `src/compaction/mod.rs`                  | **[Parity]** |
| Compaction estimate              | `harness/compaction/` | `src/compaction/estimation.rs`           | **[Parity]** |
| Branch summarization             | `harness/compaction/` | `src/compaction/branch_summarization.rs` | **[Parity]** |
| Fresh routing session IDs        | v0.82.0               | `CheckpointTail` mechanism               | **[Parity]** |
| Split-turn summary serialization | `harness/compaction/` | `src/compaction/compact.rs`              | **[Parity]** |

### Elph-only Extensions (not in pi-agent-core)

| Feature                  | Location                            | Description                                                   |
| ------------------------ | ----------------------------------- | ------------------------------------------------------------- |
| Goals                    | `src/goals/`                        | Goal tracking with progress, budget, and completion criteria  |
| Subagent                 | `src/agent/subagent/`               | Subagent spawning and coordination                            |
| Plugins (WASM)           | `src/plugins/`                      | wasmi core-Wasm plugins (`extensions` feature)                |
| MCP client               | `src/tools/mcp/`                    | Full MCP integration: stdio/SSE/HTTP transports, auth, crypto |
| Collaboration modes      | `src/collaboration/`                | Plan mode, default mode, tool blocking                        |
| Datastore                | `src/datastore/`                    | Turso database management and migrations                      |
| Skills                   | `src/skills/`                       | Skill-based prompt injection and template system              |
| Prompt encoding (TOON)   | `src/prompt/encoding/`              | Token-optimized prompt encoding                               |
| Sandbox                  | `src/sandbox/`                      | Sandboxed execution environment                               |
| Session directory layout | `src/session/backends/session_dir/` | Multi-file session directory backend                          |
| Hyper provider           | `crates/elph-ai`                    | Elph-only provider with OAuth                                 |

---

## Tag Legend

| Tag              | Meaning                                                       |
| ---------------- | ------------------------------------------------------------- |
| **[Parity]**     | Feature is implemented on both sides with equivalent behavior |
| **[Partial]**    | Feature exists in the port but is incomplete vs mainstream    |
| **[Gap P1]**     | User-visible gap — provider or agent loop behavior            |
| **[Gap P2]**     | Polish, edge cases, optional interop                          |
| **[Elph delta]** | Intentional extension absent from upstream                    |
| **[N/A]**        | Platform-specific; not a 1:1 port target                      |
