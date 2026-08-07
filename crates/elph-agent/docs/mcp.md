# MCP integration

`elph-agent` embeds an MCP **client** (via [rmcp](https://crates.io/crates/rmcp)) so the agent can call tools exposed by external MCP servers.

Feature flag: **`mcp`** (enabled by default).

## Configuration

JSON file (Elph product: `~/.elph/mcp.json`):

```json
{
    "servers": {
        "filesystem": {
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
            "env": {},
            "timeoutMs": 60000
        },
        "remote": {
            "type": "http",
            "url": "https://mcp.example.com/mcp",
            "authTokenEnv": "MCP_REMOTE_TOKEN",
            "headers": {
                "X-App": "elph"
            },
            "timeoutMs": 45000
        },
        "deepwiki": {
            "type": "http",
            "url": "https://deepwiki.example.com/mcp",
            "lifecycle": "legacy"
        },
        "off": {
            "type": "stdio",
            "command": "unused",
            "disabled": true
        }
    }
}
```

| Field                                              | Transports | Description                                                                                                                            |
| -------------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `type`                                             | all        | `stdio`, `http` (preferred remote), or deprecated `sse`                                                                                |
| `command` / `args` / `env` / `cwd`                 | stdio      | Child process                                                                                                                          |
| `url` / `headers` / `authToken` / `authTokenEnv`   | http/sse   | Remote endpoint                                                                                                                        |
| `oauth` / `oauthScopes` / `oauthClientMetadataUrl` | http/sse   | OAuth 2.1 + PKCE; CIMD URL preferred over DCR                                                                                          |
| `timeoutMs`                                        | all        | Per list/call timeout (default 60s)                                                                                                    |
| `enable`                                           | all        | Skip when false                                                                                                                        |
| `lifecycle`                                        | all        | `auto` (default), `legacy`, or `discover` (2026-07-28). Set `legacy` for servers that reject unknown methods with non-standard errors. |
| `mrtrElicitation`                                  | all        | `decline` (default) or `error` for SEP-2322 elicitation during tool calls                                                              |

## API surface

| Type / fn                             | Role                                  |
| ------------------------------------- | ------------------------------------- |
| `McpConfig`                           | Root config                           |
| `McpServerConfig`                     | `Stdio` \| `Http`                     |
| `McpLoadOptions`                      | Fail-open load, concurrency           |
| `McpToolRegistry::load`               | Discover tools (default fail-open)    |
| `McpToolRegistry::create_agent_tools` | `mcp_{server}__{tool}` agent tools    |
| `McpSessionPool`                      | Long-lived connections with reconnect |
| `probe_server`                        | Connectivity check for doctor/CLI     |

### Tool naming

Exposed names: `mcp_{sanitized_server}__{sanitized_tool}`
Helpers: `expose_tool_name`, `parse_exposed_tool_name`.

### Production behavior

1. **Load** runs discovery concurrently (default max 4). Failed servers are logged and skipped (`continue_on_error: true`).
2. **Calls** use a **session pool**: one stdio process / HTTP session per server, mutexed, with **one automatic reconnect** on failure.
3. **Timeouts** apply per operation (list tools, call tool).
4. **Shutdown**: `McpToolRegistry::shutdown` or drop of session pool closes clients.

## Usage

```rust
use elph_agent::{McpConfig, McpLoadOptions, McpServerConfig, McpToolRegistry};
use std::sync::Arc;

let mut config = McpConfig::default();
config.servers.insert(
    "fs".into(),
    McpServerConfig::stdio("npx", vec![
        "-y".into(),
        "@modelcontextprotocol/server-filesystem".into(),
        "/tmp".into(),
    ]),
);

let registry = Arc::new(
    McpToolRegistry::load_with_options(config, McpLoadOptions::default()).await?,
);
let mut tools = elph_agent::BuiltinToolsBuilder::new(env).without_web().build();
tools.extend(registry.create_agent_tools().await);
// pass tools into AgentHarness / AgentLoop
```

Elph app wiring: `crates/coding-agent/src/agent/runtime.rs` loads `mcp.json` and extends the tool list.

## Sealed auth store (zero-trust)

`auth.json` uses per-field `enc:` encryption (AES-256-GCM). The master key is wrapped
with a machine-derived key and persisted at `DATA_DIR/auth.lock`
(default `~/.local/share/elph/auth.lock`) — no OS keychain, no user passphrase. The
wrapping key is derived via HKDF-SHA256 from this machine's hardware UUID / machine-id.

Legacy cleartext stores are not migrated; re-authenticate providers/MCP.

### String helpers (`enc:`)

Still available for ad-hoc secrets (optional key file via `Aes256Key::load_or_create`).

| Function                                                         | Role                    |
| ---------------------------------------------------------------- | ----------------------- |
| `load_or_create_master_key` / `set_process_master_key_for_tests` | Auth-store master key   |
| `encrypt_string_async` / `decrypt_string_async`                  | UTF-8 string round-trip |
| `is_encrypted_value`                                             | Detect `enc:` prefix    |

```rust
use std::sync::Arc;
use elph_agent::{Aes256Key, encrypt_string_async, decrypt_string_async, is_encrypted_value};

let key = Arc::new(Aes256Key::generate());
let cipher = encrypt_string_async(Arc::clone(&key), "my-secret-token").await?;
assert!(is_encrypted_value(&cipher));
let plain = decrypt_string_async(key, cipher).await?;
assert_eq!(plain, "my-secret-token");
```

### Example CLI

```sh
# Interactive demo (round-trip + nonce + JSON)
cargo run -p elph-agent --features mcp --example encrypt_string -- demo

# Encrypt / decrypt with a key file
cargo run -p elph-agent --features mcp --example encrypt_string -- \
  encrypt --key /tmp/elph.key --text "hello secret"

cargo run -p elph-agent --features mcp --example encrypt_string -- \
  decrypt --key /tmp/elph.key --cipher 'enc:…'

# JSON object
cargo run -p elph-agent --features mcp --example encrypt_string -- \
  encrypt-json --key /tmp/elph.key --json '{"token":"abc"}'
```

### Tests

```sh
# Unit tests (in crypto.rs)
cargo test -p elph-agent --features mcp --lib mcp::crypto

# Integration tests
cargo test -p elph-agent --features mcp --test encrypt_string
```

Covers: unicode/empty/long strings, nonce uniqueness, wrong key, tamper detection, key reload from disk, JSON blobs, sync API.

## MCP 2026-07-28 client surface

| Spec area                                            | Elph status                                                     |
| ---------------------------------------------------- | --------------------------------------------------------------- |
| Lifecycle `server/discover` + preferred `2026-07-28` | Yes (`lifecycle` auto/legacy/discover)                          |
| Streamable HTTP + stdio                              | Yes                                                             |
| SSE (deprecated)                                     | Yes, with doctor warnings; OAuth token re-resolved on reconnect |
| Auth SEPs (iss, application_type, issuer-bound DCR)  | Via rmcp ≥ 3.0.1                                                |
| CIMD (`oauthClientMetadataUrl`)                      | Config + OAuth flow hook                                        |
| Sealed `auth.json` `{providers,mcp}`                 | Yes                                                             |
| List cache SEP-2549                                  | Yes (`McpLoadOptions.response_cache`)                           |
| MRTR elicitation                                     | Policy decline/error (no full TUI)                              |
| Tasks extension bridges                              | Yes when server advertises tasks                                |
| Resource/prompt bridges                              | Yes                                                             |
| MCP Apps / EMA / server role                         | Out of scope                                                    |

## Limitations

- MCP **server** role (hosting tools for other clients) is out of scope.
- Interactive TUI for MRTR elicitation is not implemented (`mrtrElicitation` only).
- MCP Apps and Enterprise Managed Authorization (EMA) are not exposed.
