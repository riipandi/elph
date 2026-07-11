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

```bash
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

| `type`                                        | Meaning                                   |
| --------------------------------------------- | ----------------------------------------- |
| `stdio`                                       | Local child process                       |
| `http` / `streamableHttp` / `streamable-http` | Streamable HTTP (current remote standard) |
| `sse`                                         | Legacy HTTP+SSE                           |

### Auth

- **Bearer**: `authToken` or `authTokenEnv`
- **OAuth 2.1 + PKCE**: set `"oauth": true`, then:

```bash
elph mcp auth remote
elph mcp logout remote
```

Credentials: shared file `~/.elph/auth.json` (keyed by server name under `mcp`, mode `0600` on Unix).

Each server entry is **AES-256-GCM** encrypted with prefix `enc:` (URL-safe base64 of
`nonce || ciphertext`). The 32-byte key lives next to the store as `auth.key` (also `0600`),
or can be supplied via `FileCredentialStore::with_key` / builder. Crypto runs on
`spawn_blocking` so the async runtime is not blocked.

Legacy plaintext objects are still readable and re-encrypted on the next save.

The library does not hardcode this path — hosts pass it via `AuthStorePathBuilder` /
`McpLoadOptions.auth_store_path` (default filename `auth.json`).

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

```bash
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

```bash
ELPH_MCP_LIVE=1 cargo test -p elph-agent --features mcp --test mcp_deepwiki -- --nocapture
```

Without `ELPH_MCP_LIVE=1` the network tests are skipped (safe for CI).
