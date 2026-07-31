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

| `type` | Meaning |
| ------ | ------- |
| `stdio` | Local child process |
| `http` | Streamable HTTP (preferred remote transport; MCP 2026-07-28) |
| `sse` | **Deprecated** HTTP+SSE (2024-11-05). Prefer `http`. Kept for the 12-month offramp. |

### Protocol lifecycle (MCP 2026-07-28)

Per-server field `lifecycle` (default `auto`):

| Value | Behavior |
| ----- | -------- |
| `auto` | Prefer `server/discover` with protocol `2026-07-28`, fall back to legacy `initialize` |
| `legacy` | Always use `initialize` / `notifications/initialized` |
| `discover` | Require `server/discover` only (fails on legacy-only servers) |

Client identity advertises name `elph`, protocol preference `2026-07-28`, form elicitation, and the Tasks extension. List responses use the rmcp SEP-2549 client cache (configurable via `McpLoadOptions.response_cache`).

### MRTR elicitation (SEP-2322)

`mrtrElicitation` (default `decline`):

| Value | Behavior |
| ----- | -------- |
| `decline` | Decline server elicitation during tool calls |
| `error` | Fail elicitation with a clear error for the agent |

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

The entire document is an **AES-256-GCM envelope** (`v: 2`). The master key lives only in the
**OS keychain** (zero-trust) — never as `auth.key` beside the store, and no `auth.json.lock`
sidecar. Logical payload holds MCP OAuth JSON objects and provider API keys / `env:VAR` refs.

CI/tests may inject a key via `set_process_master_key_for_tests` or `ELPH_AUTH_MASTER_KEY_B64`.

Legacy cleartext stores are **not** migrated — re-run `elph provider connect` / `mcp auth`.

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
let tools = registry.create_agent_tools();
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
