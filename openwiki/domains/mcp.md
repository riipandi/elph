---
type: Concept
title: MCP — Model Context Protocol
description: Elph's MCP client integration — transports, AES-256-GCM encryption, tool naming, session pool, and policy
tags: [mcp, model-context-protocol, rmcp, transports, encryption]
---

# MCP

Model Context Protocol (MCP) integration lives in `crates/elph-agent/src/tools/mcp/`. It uses the `rmcp` crate (v3.0.0+, upgraded from `5b658eb`) for client-side MCP communication. MCP tools are exposed as standard [Agent Tools](../domains/tools.md) and are invoked during the [Agent Loop](../workflows/agent-loop.md) turn cycle.

## Module Structure

```
crates/elph-agent/src/tools/mcp/
├── mod.rs             — module docs + re-exports
├── registry.rs        — McpToolRegistry — discover, load, bridge tools
├── client.rs          — MCP client operations (call_tool, probe_server, validate)
├── config.rs          — McpConfig, McpServerConfig, McpLoadOptions
├── auth.rs            — AuthStoreFile, FileCredentialStore, McpOAuthFlow
├── auth_resolve.rs    — OAuth token resolution
├── crypto.rs          — AES-256-GCM encryption helpers
├── compat.rs          — compatibility shims
├── events.rs          — McpServerEvent
├── policy.rs          — McpPolicyConfig, tool approval rules
├── session.rs         — McpSessionPool — connection reuse
├── sse.rs             — SSE transport support
├── store_lock.rs      — AuthStoreGuard, lock_auth_store
├── truncate.rs        — Tool result truncation limits
└── validate.rs        — Config validation (JSON Schema + semantic)
```

## Transports

MCP supports three transport types:

| Transport           | Description                                                 |
| ------------------- | ----------------------------------------------------------- |
| **stdio**           | Subprocess-based — stdin/stdout with a local server process |
| **streamable HTTP** | HTTP-based streaming (2024-11-05 protocol)                  |
| **SSE**             | Server-Sent Events transport                                |

## Session Pool

`McpSessionPool` (from `session.rs`) reuses MCP connections across tool calls:

- Stdio processes are kept alive between calls.
- HTTP sessions maintain connection pools.
- Pool is keyed by server config hash.

## Tool Naming Convention

MCP tools are exposed to the agent model as `mcp_{server}__{tool}`:

```rust
// Example: server "filesystem", tool "read_file"
// Exposed as: mcp_filesystem__read_file
```

This naming prevents collisions between MCP tools and built-in agent tools.

## AES-256-GCM Encryption

MCP credentials are encrypted at rest using AES-256-GCM (via `aes-gcm` crate):

```rust
// From crypto.rs — encrypts/decrypts credential store files
// Credentials stored with "enc:" prefix in shared auth.json
```

The `AuthStoreFile` (from `auth.rs`) manages a shared `auth.json` file with encrypted credential entries. `FileCredentialStore` implements the `CredentialStore` trait for MCP-specific credentials.

**Key wrapping:** Commit `7b7ffc2` replaced OS keychain master key with machine-bound wrapped key in `auth.lock` using `rewrap_master_key` + `flock()` for concurrent access safety.

## Key Features Added Since Last Audit

| Feature                                 | Commit    | Details                                                    |
| --------------------------------------- | --------- | ---------------------------------------------------------- |
| `rmcp` v3.0.0 upgrade                   | `5b658eb` | Refactored OAuth/client lifecycle, 2026-07-28 protocol     |
| Lazy per-server discovery               | `d07576b` | On-demand discovery with progress reporting                |
| Call-tool-once + on-demand per-server   | `65ccc01` | `call_tool_once()` replaces eager discovery                |
| Discovery retry + graceful degradation  | `2a87cf5` | Retry on transient server failures                         |
| MCP 2026-07-28 lifecycle                | `d6127b6` | MRTR elicitation, Tasks, CIMD support                      |
| Lifecycle mode support                  | `c30c2b3` | MCP server protocol lifecycle modes                        |
| Tool result cache (Turso → in-memory)   | `ba083e6` | Switched to in-memory HashMap + JSONL file for persistence |
| Rewrap master key + flock               | `7b7ffc2` | Machine-bound wrapped key, concurrent key creation safety  |
| TUI slash commands for MCP OAuth        | `ad35e32` | `/mcp:login`, `/mcp:logout` in TUI                         |
| Legacy v2 envelope support              | `703a7b4` | Compatibility with older MCP credential formats            |
| Lazy/eager load strategy (lazy default) | `70a6ffd` | Configurable per-server discovery strategy                 |

## McpToolRegistry

Defined in `registry.rs`:

```rust
pub struct McpToolRegistry {
    config: McpConfig,
    session_pool: McpSessionPool,
    load_report: Arc<RwLock<McpLoadReport>>,
}
```

Key methods:

```rust
impl McpToolRegistry {
    pub async fn load(&self) -> Result<()>;                    // discover all servers
    pub async fn load_with_options(&self, options: McpLoadOptions) -> Result<()>;
    pub fn create_agent_tools(&self) -> Vec<AgentTool>;        // bridge to agent tools
    pub fn get_report(&self) -> McpLoadReport;                  // load status
    pub async fn refresh(&self) -> Result<()>;                  // hot reload
}
```

## MCP Bootstrap Flow

1. `McpToolRegistry::load()` discovers configured servers from `mcp.json`.
2. For each server, `probe_server_with_auth()` attempts connection with credential resolution.
3. Success → `McpSessionPool` caches the connection.
4. `create_agent_tools()` wraps each MCP tool as an `AgentTool` with `mcp_{server}__{tool}` naming.
5. Policy filters deny-listed tools; `mcp_tool_requires_approval()` checks approval rules.
6. `tools/list_changed` can refresh catalogs in-place.

## Policy

`McpPolicyConfig` (from `policy.rs`) defines per-server rules:

```rust
pub struct McpPolicyConfig {
    pub allow: Option<Vec<String>>,       // tool allow list
    pub deny: Option<Vec<String>>,        // tool deny list
    pub require_approval: Option<Vec<String>>,  // tools needing user approval
}
```

## Source References

- `crates/elph-agent/src/tools/mcp/mod.rs` — module documentation
- `crates/elph-agent/src/tools/mcp/registry.rs` — `McpToolRegistry`, `McpToolDescriptor`, `McpLoadReport`
- `crates/elph-agent/src/tools/mcp/client.rs` — `call_tool_for_server()`, `probe_server_with_auth()`
- `crates/elph-agent/src/tools/mcp/config.rs` — `McpConfig`, `McpServerConfig`
- `crates/elph-agent/src/tools/mcp/auth.rs` — `AuthStoreFile`, `FileCredentialStore`
- `crates/elph-agent/src/tools/mcp/crypto.rs` — AES-256-GCM encryption
- `crates/elph-agent/src/tools/mcp/session.rs` — `McpSessionPool`
- `crates/elph-agent/src/tools/mcp/policy.rs` — `McpPolicyConfig`, tool approval
