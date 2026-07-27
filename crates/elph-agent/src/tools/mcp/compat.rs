//! Compatibility shims for editor-style MCP config (Cursor, VS Code, Claude Code).

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde_json::Value;

/// Normalize editor-style MCP JSON into Elph's canonical shape before schema validation.
///
/// - `servers` → `mcpServers` (when `mcpServers` is absent, old format)
/// - infer `type: "http"` when a server has `url` but no `type`
/// - infer `type: "stdio"` when a server has `command` but no `type`
pub fn normalize_mcp_config_value(mut value: Value) -> Value {
    let Some(root) = value.as_object_mut() else {
        return value;
    };

    // If the old `servers` key is used, rename it to `mcpServers`.
    if !root.contains_key("mcpServers")
        && let Some(servers) = root.remove("servers")
    {
        root.insert("mcpServers".to_string(), servers);
    }

    if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        for server in servers.values_mut() {
            normalize_server_entry(server);
        }
    }

    value
}

fn normalize_server_entry(server: &mut Value) {
    let Some(entry) = server.as_object_mut() else {
        return;
    };
    if entry.contains_key("type") {
        return;
    }
    if entry.contains_key("url") {
        entry.insert("type".to_string(), Value::String("http".to_string()));
    } else if entry.contains_key("command") {
        entry.insert("type".to_string(), Value::String("stdio".to_string()));
    }
}

/// Resolve a configured HTTP header value (supports `env.VAR` and `${env:VAR}` placeholders).
pub fn resolve_mcp_header_value(raw: &str) -> Result<String> {
    if let Some(var) = raw.strip_prefix("env.") {
        return std::env::var(var).with_context(|| format!("environment variable `{var}` not set (MCP header)"));
    }
    if let Some(inner) = raw.strip_prefix("${env:").and_then(|rest| rest.strip_suffix('}')) {
        return std::env::var(inner).with_context(|| format!("environment variable `{inner}` not set (MCP header)"));
    }
    Ok(raw.to_string())
}

/// Resolve all configured HTTP headers for transport setup.
pub fn resolve_http_headers(headers: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let mut resolved = BTreeMap::new();
    for (key, value) in headers {
        resolved.insert(key.clone(), resolve_mcp_header_value(value)?);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_mcp_servers_and_infers_http_type() {
        let raw = json!({
            "mcpServers": {
                "deepwiki": { "url": "https://mcp.deepwiki.com/mcp" },
                "lightpanda": {
                    "command": "lightpanda",
                    "args": ["mcp", "--obey-robots"],
                    "type": "stdio"
                }
            }
        });
        let normalized = normalize_mcp_config_value(raw);
        let servers = normalized["mcpServers"].as_object().expect("mcpServers");
        assert_eq!(servers["deepwiki"]["type"], "http");
        assert_eq!(servers["lightpanda"]["type"], "stdio");
    }

    #[test]
    fn normalizes_old_servers_key_to_mcp_servers() {
        let raw = json!({
            "servers": {
                "native": { "url": "https://example.com/mcp" }
            }
        });
        let normalized = normalize_mcp_config_value(raw);
        assert!(normalized.get("servers").is_none());
        let servers = normalized["mcpServers"].as_object().expect("mcpServers");
        assert_eq!(servers["native"]["type"], "http");
    }

    #[test]
    fn prefers_mcp_servers_when_both_present() {
        let raw = json!({
            "servers": { "ignored": { "url": "https://ignored.example/mcp" } },
            "mcpServers": { "native": { "type": "http", "url": "https://example.com/mcp" } }
        });
        let normalized = normalize_mcp_config_value(raw);
        let servers = normalized["mcpServers"].as_object().expect("mcpServers");
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("native"));
    }

    #[test]
    fn resolves_env_header_placeholder() {
        // SAFETY: test-only env mutation; single-threaded test harness.
        unsafe {
            std::env::set_var("ELPH_MCP_TEST_HEADER", "secret");
        }
        let value = resolve_mcp_header_value("env.ELPH_MCP_TEST_HEADER").expect("resolve");
        assert_eq!(value, "secret");
        unsafe {
            std::env::remove_var("ELPH_MCP_TEST_HEADER");
        }
    }
}
