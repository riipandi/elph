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

| Field                                            | Transports | Description                          |
| ------------------------------------------------ | ---------- | ------------------------------------ |
| `type`                                           | both       | `stdio` or `http`                    |
| `command` / `args` / `env` / `cwd`               | stdio      | Child process                        |
| `url` / `headers` / `authToken` / `authTokenEnv` | http       | Streamable HTTP endpoint             |
| `timeoutMs`                                      | both       | Per list/call timeout (default 60s)  |
| `disabled`                                       | both       | Skip during discovery and calls      |
| `lifecycle`                                      | both       | `auto` (default), `legacy`, or `discover`. Auto probes `server/discover` and falls back to `initialize`; `legacy` uses the old handshake only; `discover` requires 2026-07-28+. Set to `legacy` for servers (e.g. DeepWiki) that reject unknown methods with non-standard errors. |

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
tools.extend(registry.create_agent_tools());
// pass tools into AgentHarness / AgentLoop
```

Elph app wiring: `elph/src/agent/runtime.rs` loads `mcp.json` and extends the tool list.

## Sealed auth store (zero-trust)

`auth.json` is an **AES-256-GCM envelope** (`v: 2`). Master key lives only in the **OS keychain**
(`space.elph.auth` / `auth-store-master-v2`) — never `auth.key` on disk. No `auth.json.lock`.

Legacy cleartext stores are not migrated; re-authenticate providers/MCP.

### String helpers (`enc:`)

Still available for ad-hoc secrets (optional key file via `Aes256Key::load_or_create`).

| Function | Role |
| -------- | ---- |
| `load_or_create_master_key` / `set_process_master_key_for_tests` | Auth-store master key |
| `encrypt_string_async` / `decrypt_string_async` | UTF-8 string round-trip |
| `is_encrypted_value` | Detect `enc:` prefix |

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

## Limitations

- MCP **server** role (hosting tools for other clients) is out of scope.
- OAuth browser login for remote MCP is not fully productized (token via `authToken` / `authTokenEnv`).
- Resource/prompt MCP surfaces are not yet mapped to agent tools (tools only).
- Tasks and Apps (2026-07-28 Extensions framework) are not yet exposed.
