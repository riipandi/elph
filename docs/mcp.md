# MCP integration

Elph connects to [Model Context Protocol](https://modelcontextprotocol.io/) servers and exposes their tools (plus resources/prompts bridges) to the agent loop.

## Config

Schema: [`schemas/mcp-schema.json`](../schemas/mcp-schema.json).

| Layer       | Path                       | Role                                       |
| ----------- | -------------------------- | ------------------------------------------ |
| **Home**    | `~/.elph/mcp.json`         | Global servers; default for `elph mcp add` |
| **Project** | `<project>/.elph/mcp.json` | Overrides / extra servers for this repo    |

Runtime loads **home**, then merges **project** on top (same server name → project wins).
Policy maps are merged the same way as per-server policy overlays.

Tool results are truncated (~32k chars per text block) before they enter the agent context.
Optional [TOON prompt encoding](../crates/elph-agent/docs/prompt-encoding.md) can further compress large `structured_content` payloads (e.g. DeepWiki) in model-visible tool results when `ELPH_PROMPT_ENCODING=toon` or `auto`.
OAuth tokens live in encrypted `auth.json` (`enc:…`); CLI never prints secrets.
SSE remotes can use OAuth the same way as Streamable HTTP.

### Tool loading strategy

Per-server field `loadStrategy` (default `lazy`):

| Value   | Behavior                                                                                                                                                                                                                                                                                                          |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lazy`  | **(default)** Skip `tools/list` (and resources/prompts discovery) at load time. Tools are discovered on-demand per-server: first `create_agent_tools`, `call_tool`, `read_resource`, or `get_prompt` triggers discovery only for that server. Results are merged into the catalog — other servers stay untouched. |
| `eager` | Legacy behavior — list all catalogs during `McpToolRegistry::load`.                                                                                                                                                                                                                                               |

**Key improvements:** lazy-loaded servers now correctly discover on their first tool call:

- **Graceful degradation:** `create_agent_tools()` returns already-discovered tools even when discovery has errors — never returns empty.
- **Retry:** `ensure_server_discovered()` retries once on transient failure before giving up.
- **Merge, not replace:** `discover_server()` and `discover_tools_with_options()` merge results into the existing catalog instead of replacing all tools.
- **Partial attach:** TUI bootstrap (`bootstrap_mcp_for_session`) always attaches tools even when some servers fail — partial results are better than no tools.
- **No stale refresh:** The old code called `refresh_server()` (drops and re-creates the session) on every tool invocation. Now `ensure_server_discovered()` fires discovery exactly once per server, after which the pooled session handles all subsequent calls.
- **Pre-turn sweep:** before every agent turn the session calls `ensure_mcp_tools_ready()`, which discovers any enabled server still pending and hot-attaches the new tools to the harness. A lazy server that was skipped at startup (or failed earlier) is thus available to the model on the very next turn without a restart.

**Transport note:** `call_tool` now uses `call_tool_once` (non-MRTR) internally, which works with all transports (stdio, HTTP, SSE). The MRTR-aware `call_tool()` requires HTTP transport and fails on stdio with `"Requires HTTP transport (--port)"`. Since the agent harness doesn't support interactive MRTR rounds (the default `MrtrElicitationPolicy::Decline` declines all elicitation), `call_tool_once` is the correct choice.

```json
{
    "mcpServers": {
        "fs": {
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
            "loadStrategy": "eager"
        },
        "deepwiki": {
            "type": "http",
            "url": "https://mcp.deepwiki.com/mcp"
        }
    }
}
```

The global default is `lazy`. Set a server to `eager` when you need its tools available immediately (e.g. for early system-prompt compilation).

### Credential conflict: env vs `auth.json`

If both a static bearer (`authToken` / `authTokenEnv`) **and** an OAuth entry in `auth.json`
exist for the same server, connection fails unless you set `authConflict`:

| `authConflict`    | Behavior                                                    |
| ----------------- | ----------------------------------------------------------- |
| `error` (default) | Fail with a clear message                                   |
| `preferEnv`       | Use env/inline bearer; warn that auth.json is ignored       |
| `preferOauth`     | Use auth.json OAuth (refreshable); warn that env is ignored |

```json
{
    "servers": {
        "api": {
            "type": "http",
            "url": "https://example.com/mcp",
            "authTokenEnv": "MCP_TOKEN",
            "oauth": true,
            "authConflict": "preferEnv"
        }
    }
}
```

`elph mcp doctor` reports `auth=… CONFLICT(policy=…)` without printing secret values.

```sh
# Project-only DeepWiki (does not touch home config)
elph mcp add --project deepwiki '{"type":"http","url":"https://mcp.deepwiki.com/mcp"}'

elph mcp list                 # merged view with [home] / [project] tags
elph mcp list --project       # project layer only
elph mcp remove --project deepwiki
elph mcp remove --all name    # both layers
```

```json
{
    "policy": {
        "default": "requireApproval",
        "allow": ["mcp_fs__list*", "mcp_fs__read*"],
        "deny": ["mcp_dangerous__*"]
    },
    "servers": {
        "fs": {
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        },
        "remote": {
            "type": "http",
            "url": "https://example.com/mcp",
            "oauth": true
        },
        "legacy": {
            "type": "sse",
            "url": "http://localhost:3000/sse"
        }
    }
}
```

### Transports

| `type`  | Meaning                                                                             |
| ------- | ----------------------------------------------------------------------------------- |
| `stdio` | Local child process                                                                 |
| `http`  | Streamable HTTP (preferred remote transport; MCP 2026-07-28)                        |
| `sse`   | **Deprecated** HTTP+SSE (2024-11-05). Prefer `http`. Kept for the 12-month offramp. |

### Protocol lifecycle (MCP 2026-07-28)

Per-server field `lifecycle` (default `auto`):

| Value      | Behavior                                                                              |
| ---------- | ------------------------------------------------------------------------------------- |
| `auto`     | Prefer `server/discover` with protocol `2026-07-28`, fall back to legacy `initialize` |
| `legacy`   | Always use `initialize` / `notifications/initialized`                                 |
| `discover` | Require `server/discover` only (fails on legacy-only servers)                         |

Client identity advertises name `elph`, protocol preference `2026-07-28`, form elicitation, and the Tasks extension. List responses use the rmcp SEP-2549 client cache (configurable via `McpLoadOptions.response_cache`).

### Tool result cache

Read-only tool call results are cached so repeated calls with the same arguments return instantly instead of hitting the MCP server again. The cache is an **in-memory HashMap persisted to a JSONL file** (one JSON object per line) — no database engine involved.

**Storage** (per `docs/configuration.md` layout):

| Scope   | Path                                                         |
| ------- | ------------------------------------------------------------ |
| Session | `APP_DATA/sessions/<SESSION_ID>/mcp_cache/cache.jsonl`       |
| Host    | `APP_DATA/mcp_cache/cache.jsonl` (CLI ops without a session) |

**Behavior:**

- **Cache key** — hash of `(server, tool, canonical JSON args)`. Different args → different entries.
- **Read-only only** — tools whose names contain mutation keywords (`write`, `create`, `delete`, `update`, `edit`, `set`, `add`, `remove`, …) are never cached.
- **TTL** — default 60 seconds. Precedence: per-server `cacheTtlMs` in `mcp.json` → global `settings.json` `mcp.cacheTtlSecs` → 60s default. `0` disables caching.
- **Max entries** — `settings.json` `mcp.cacheMaxEntries` (default 2048). Expired entries are pruned when over the limit.
- **Persistence** — entries survive restarts (loaded from JSONL on open). File is rewritten atomically (temp + rename) on eviction/invalidation/clear.
- **Invalidation** — entries for a server are dropped on reconnect; `elph mcp` reload clears the store.

```json
{
    "mcpServers": {
        "deepwiki": {
            "type": "http",
            "url": "https://mcp.deepwiki.com/mcp",
            "cacheTtlMs": 300000
        }
    }
}
```

Global retention in `settings.json`:

```json
{
    "mcp": {
        "cacheTtlSecs": 60,
        "cacheMaxEntries": 2048
    }
}
```

Library hosts opt in by opening a `McpCacheStore` and passing it via `McpLoadOptions.cache_store`.

### MRTR elicitation (SEP-2322)

`mrtrElicitation` (default `decline`):

| Value     | Behavior                                          |
| --------- | ------------------------------------------------- |
| `decline` | Decline server elicitation during tool calls      |
| `error`   | Fail elicitation with a clear error for the agent |

Interactive TUI elicitation is not implemented; use `decline`/`error` for deterministic agent runs.

### Auth

- **Bearer**: `authToken` or `authTokenEnv`
- **OAuth 2.1 + PKCE**: set `"oauth": true`, then:

```sh
elph mcp auth remote
elph mcp logout remote
```

Auth SEPs from MCP 2026-07-28 (RFC 9207 `iss`, `application_type`, issuer-bound DCR credentials) are enforced by **rmcp ≥ 3.0.1**. Prefer **CIMD** when you have a public client metadata URL:

```json
{
    "type": "http",
    "url": "https://example.com/mcp",
    "oauth": true,
    "oauthClientMetadataUrl": "https://your.app/.well-known/oauth-client"
}
```

DCR remains available for backward compatibility (spec-deprecated, still supported).

Credentials: sealed file `CONFIG_DIR/auth.json` (default `~/.config/elph/auth.json`).

The document is plain JSON with per-field `enc:` encryption (AES-256-GCM). The master
key is wrapped with a machine-derived key and persisted at `DATA_DIR/auth.lock`
(default `~/.local/share/elph/auth.lock`) — no OS keychain, no user passphrase. The
wrapping key is derived via HKDF-SHA256 from this machine's hardware UUID / machine-id,
so copying `auth.json` + `auth.lock` to another machine will not decrypt. Logical payload
holds MCP OAuth JSON objects and provider API keys / `env:VAR` refs.

CI/tests may inject a key via `set_process_master_key_for_tests` or `ELPH_AUTH_KEY`.

Hosts pass the path via `AuthStorePathBuilder` / `McpLoadOptions.auth_store_path`.

### Config validation

`mcp.json` is validated on load against `schemas/mcp-schema.json` plus semantic checks
(empty command, invalid URL scheme, empty policy patterns). Invalid files fail with a
clear multi-error message instead of being half-applied.

## CLI

| Command                            | Behavior                            |
| ---------------------------------- | ----------------------------------- |
| `elph mcp list`                    | Servers + oauth status (no secrets) |
| `elph mcp add <name> <json\|file>` | Upsert server                       |
| `elph mcp remove <name>`           | Remove server + OAuth creds         |
| `elph mcp doctor`                  | Probe connectivity                  |
| `elph mcp auth <name>`             | OAuth browser flow                  |
| `elph mcp logout <name>`           | Clear OAuth tokens                  |

## TUI slash commands

Same sealed `auth.json` store as the CLI / `/provider connect`.

| Command                       | Behavior                                                             |
| ----------------------------- | -------------------------------------------------------------------- |
| `/mcp` or `/mcp list`         | List merged servers (home + project) + OAuth status                  |
| `/mcp auth`                   | Open MCP OAuth dialog — pick a remote server                         |
| `/mcp auth figma`             | Prefill/filter; **auto-starts** OAuth when the name matches uniquely |
| `/mcp login` / `/mcp connect` | Aliases for `auth`                                                   |
| `/mcp logout <name>`          | Clear OAuth tokens for that server                                   |

Example Figma entry in `~/.config/elph/mcp.json` (or project `.elph/mcp.json`):

```json
{
    "mcpServers": {
        "figma": {
            "type": "http",
            "url": "https://mcp.figma.com/mcp",
            "oauth": true,
            "oauthScopes": ["mcp:connect"],
            "oauthClientName": "Elph MCP Client"
        }
    }
}
```

Then in the TUI: `/mcp auth figma` → browser PKCE → tokens sealed under `auth.json` → `mcp.figma`.

**OAuth discovery:** the client loads protected-resource + authorization-server metadata (RFC 9728 / 8414) and installs it on the OAuth manager before dynamic registration (DCR). If DCR is rejected (some hosts only allowlist known clients), set `oauthClientId` / `oauthClientSecret` or `oauthClientMetadataUrl` (CIMD).

## Agent surface

Tools are named `mcp_{server}__{tool}` (sanitized).

Bridge tools (when the server supports the capability):

- `mcp_{server}__list_resources`
- `mcp_{server}__read_resource`
- `mcp_{server}__list_prompts`
- `mcp_{server}__get_prompt`
- **Tasks (SEP-2663)** when the server advertises `io.modelcontextprotocol/tasks`:
    - `mcp_{server}__tasks_get` — poll task by `taskId`
    - `mcp_{server}__tasks_update` — deliver `inputResponses`
    - `mcp_{server}__tasks_cancel` — cancel task

If a tool call returns `resultType: "task"`, the agent result includes `taskId` and a hint to poll with `tasks_get`.

### Policy

- **deny** — not exposed
- **allow** — exposed, no approval
- **requireApproval** (default) — exposed; TUI approval dialog (unless Brave mode)

Patterns: exact, `prefix*`, `*suffix`, `*`.

### Hot reload

When a server sends `notifications/tools/list_changed` (or resource/prompt variants), the registry refreshes that server and updates harness tools.

## Library

```rust
use elph_agent::{McpConfig, McpLoadOptions, McpToolRegistry};

let mut options = McpLoadOptions::default();
options.auth_store_path = Some(paths.auth_store_path());
let registry = McpToolRegistry::load_with_options(config, options).await?;
let tools = registry.create_agent_tools().await;
```

## Example: DeepWiki (public, no auth)

DeepWiki is a free remote MCP server for public GitHub documentation
([docs](https://docs.devin.ai/work-with-devin/deepwiki-mcp)).

**Endpoint (Streamable HTTP):** `https://mcp.deepwiki.com/mcp`
(SSE `/sse` is deprecated.)

### `~/.elph/mcp.json`

```json
{
    "servers": {
        "deepwiki": {
            "type": "http",
            "url": "https://mcp.deepwiki.com/mcp",
            "timeoutMs": 120000
        }
    }
}
```

### Run the example

```sh
cargo run -p elph-agent --features mcp --example mcp_deepwiki

# Structure for another repo
cargo run -p elph-agent --features mcp --example mcp_deepwiki -- \
  --repo rust-lang/rust --tool read_wiki_structure

# Ask a grounded question
cargo run -p elph-agent --features mcp --example mcp_deepwiki -- \
  --tool ask_question \
  --repo modelcontextprotocol/rust-sdk \
  --question "How does Streamable HTTP transport work?"
```

### Live integration tests

```sh
ELPH_MCP_LIVE=1 cargo test -p elph-agent --features mcp --test mcp_deepwiki -- --nocapture
```

Without `ELPH_MCP_LIVE=1` the network tests are skipped (safe for CI).
