---
type: Guide
title: MCP Integration
description: Model Context Protocol (MCP) server configuration, authentication, tool registration, and credential management in Elph.
tags: [mcp, integration, tools, auth, configuration]
resource: /crates/elph-agent/src/tools/mcp/
---

# MCP Integration

Elph connects to [Model Context Protocol](https://modelcontextprotocol.io/) servers and exposes their tools to the agent loop. MCP is the primary extension mechanism for adding domain-specific capabilities.

**Schema:** `/schemas/mcp-schema.json`

## Configuration layers

| Layer       | Path                       | Role                                              |
| ----------- | -------------------------- | ------------------------------------------------- |
| **Home**    | `~/.elph/mcp.json`         | Global servers; default target for `elph mcp add` |
| **Project** | `<project>/.elph/mcp.json` | Per-project overrides / extras                    |

Runtime loads **home** first, then deep-merges **project** on top. Same server name → project wins. Policy maps are merged the same way.

**Source:** `crates/elph-agent/src/tools/mcp/config.rs`

```sh
# Add a project-only DeepWiki server
elph mcp add --project deepwiki '{"type":"http","url":"https://mcp.deepwiki.com/mcp"}'

# List merged view with layer tags
elph mcp list

# List project layer only
elph mcp list --project

# Remove from specific or all layers
elph mcp remove --project deepwiki
elph mcp remove --all name
```

## Transports

| Transport             | Type config                                       | Use case                                  |
| --------------------- | ------------------------------------------------- | ----------------------------------------- |
| **stdio**             | `{"type":"stdio","command":"npx","args":["..."]}` | Local servers (filesystem, git, etc.)     |
| **HTTP** (Streamable) | `{"type":"http","url":"..."}`                     | Remote servers with SSE or streaming HTTP |
| **SSE**               | `{"type":"http","url":"...","transport":"sse"}`   | Remote servers using server-sent events   |

**Source:** `crates/elph-agent/src/tools/mcp/client.rs`, `crates/elph-agent/src/tools/mcp/sse.rs`

## Authentication

| Method                | Config field                | Auth storage                              |
| --------------------- | --------------------------- | ----------------------------------------- |
| Bearer token (env)    | `authTokenEnv: "MCP_TOKEN"` | Environment variable                      |
| Bearer token (inline) | `authToken: "sk-..."`       | Config file (not recommended for secrets) |
| OAuth 2.1 + PKCE      | `oauth: true`               | Encrypted `auth.json` under home config   |

**Credential conflict resolution** — if both a static bearer and OAuth entry exist:

| `authConflict`    | Behavior                                  |
| ----------------- | ----------------------------------------- |
| `error` (default) | Fail with clear message                   |
| `preferEnv`       | Use env/inline bearer; warn OAuth ignored |
| `preferOauth`     | Use OAuth (refreshable); warn env ignored |

OAuth tokens live in encrypted `auth.json` (`enc:...`). The CLI never prints secret values. `elph mcp doctor` reports `auth=... CONFLICT(policy=...)` without secrets.

**Source:** `crates/elph-agent/src/tools/mcp/mod.rs`, `/elph/src/cli/mcp.rs`

## Policy engine

Per-server tool policies control which tools the agent can call and whether they need approval:

```json
{
    "policy": {
        "deepwiki__search": "allow",
        "deepwiki__read": "allow",
        "filesystem__write": "approve"
    }
}
```

Policy values: `allow` (auto-execute), `approve` (require user approval), `deny` (block).

## Tool registry

During session bootstrap, the [`mcp_bootstrap`](../architecture/source-map.md#elph-binary--library-crate--elph) module discovers MCP servers from config, connects to each, and auto-registers tools with prefixed names: `{server}__{tool}`.

**Source:** `/elph/src/agent/mcp_bootstrap.rs`

## Tool result handling

- Text blocks are truncated (~32k chars) before entering agent context
- Optional [TOON encoding](agent-runtime.md#toon-prompt-encoding) further compresses large `structured_content` payloads
- Notifications are sent back to `elph mcp` listening mode when enabled

## Bypassing MCP

Set `ELPH_MCP_DISABLED=1` or use `elph run --no-mcp` to skip all MCP discovery.
Use `ELPH_MCP_FETCH_TIMEOUT_SECS` to configure the per-server connection timeout.
