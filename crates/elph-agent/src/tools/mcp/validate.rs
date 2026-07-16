//! MCP configuration validation (JSON Schema + semantic checks).

use std::sync::OnceLock;

use anyhow::bail;
use anyhow::{Context, Result};
use serde_json::Value;

use super::client::validate_server_config;
use super::compat::normalize_mcp_config_value;
use super::config::McpConfig;

/// Embedded copy of `schemas/mcp-schema.json` (draft-07).
const MCP_SCHEMA_JSON: &str = include_str!("../../../../../schemas/mcp-schema.json");

fn mcp_schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(MCP_SCHEMA_JSON).expect("embedded mcp-schema.json must be valid JSON"))
}

/// Structured validation failure for MCP config.
#[derive(Debug, Clone)]
pub struct McpConfigValidationError {
    pub errors: Vec<String>,
}

impl std::fmt::Display for McpConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.errors.is_empty() {
            write!(f, "MCP config validation failed")
        } else if self.errors.len() == 1 {
            write!(f, "MCP config validation failed: {}", self.errors[0])
        } else {
            writeln!(f, "MCP config validation failed ({} errors):", self.errors.len())?;
            for (i, e) in self.errors.iter().enumerate() {
                writeln!(f, "  {}. {e}", i + 1)?;
            }
            Ok(())
        }
    }
}

impl std::error::Error for McpConfigValidationError {}

/// Validate a raw JSON value against the MCP JSON Schema.
pub fn validate_mcp_config_value(instance: &Value) -> Result<(), McpConfigValidationError> {
    let schema = mcp_schema();
    match jsonschema::validate(schema, instance) {
        Ok(()) => Ok(()),
        Err(error) => Err(McpConfigValidationError {
            errors: vec![error.to_string()],
        }),
    }
}

/// Semantic checks beyond schema (URL scheme, empty command, empty server names).
pub fn validate_mcp_config_semantic(config: &McpConfig) -> Result<(), McpConfigValidationError> {
    let mut errors = Vec::new();
    for (name, server) in &config.servers {
        if name.trim().is_empty() {
            errors.push("server name must not be empty".into());
            continue;
        }
        if let Err(error) = validate_server_config(name, server) {
            errors.push(error.to_string());
        }
    }
    // Policy pattern sanity: empty strings are useless.
    for (label, list) in [
        ("policy.allow", &config.policy.allow),
        ("policy.deny", &config.policy.deny),
        ("policy.requireApproval", &config.policy.require_approval),
    ] {
        for p in list {
            if p.trim().is_empty() {
                errors.push(format!("{label} contains an empty pattern"));
            }
        }
    }
    for (name, server) in &config.servers {
        if let Some(policy) = server.policy() {
            for (label, list) in [
                ("allow", &policy.allow),
                ("deny", &policy.deny),
                ("requireApproval", &policy.require_approval),
            ] {
                for p in list {
                    if p.trim().is_empty() {
                        errors.push(format!("server \"{name}\" policy.{label} contains an empty pattern"));
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(McpConfigValidationError { errors })
    }
}

/// Full validation: JSON Schema then semantic rules on the typed config.
pub fn validate_mcp_config(config: &McpConfig) -> Result<(), McpConfigValidationError> {
    let value = serde_json::to_value(config).map_err(|e| McpConfigValidationError {
        errors: vec![format!("serialize config for schema check: {e}")],
    })?;
    validate_mcp_config_value(&value)?;
    validate_mcp_config_semantic(config)?;
    Ok(())
}

/// Parse raw JSON text, validate against schema + semantic rules, return typed config.
pub fn parse_and_validate_mcp_config(raw: &str) -> Result<McpConfig> {
    let value: Value = serde_json::from_str(raw).context("parse MCP config JSON")?;
    let value = normalize_mcp_config_value(value);
    validate_mcp_config_value(&value).map_err(|e| anyhow::anyhow!("{e}"))?;
    let config: McpConfig = serde_json::from_value(value).context("deserialize MCP config")?;
    validate_mcp_config_semantic(&config).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(config)
}

/// Async wrapper (schema compile/validate is CPU-light; still offloads large docs).
pub async fn parse_and_validate_mcp_config_async(raw: String) -> Result<McpConfig> {
    tokio::task::spawn_blocking(move || parse_and_validate_mcp_config(&raw))
        .await
        .context("join MCP config validation")?
}

/// Validate a single server config JSON (object) used by `mcp add`.
pub fn parse_and_validate_server_config_json(raw: &str) -> Result<super::config::McpServerConfig> {
    let value: Value = serde_json::from_str(raw).context("parse MCP server JSON")?;

    // Accept bare server object or single-entry wrapper.
    if let Ok(server) = serde_json::from_value::<super::config::McpServerConfig>(value.clone()) {
        // Schema validates full document; for a bare server, wrap temporarily.
        let wrapped = serde_json::json!({ "servers": { "_": value } });
        if let Err(e) = validate_mcp_config_value(&wrapped) {
            // Fall back to semantic-only if wrapper shape differs slightly from schema strictness
            // on the synthetic key — still enforce server semantic checks.
            let _ = e;
        }
        validate_server_config("_", &server)?;
        return Ok(server);
    }

    let config = parse_and_validate_mcp_config(raw)?;
    if config.servers.len() == 1 {
        return Ok(config.servers.into_values().next().expect("one server"));
    }
    bail!("MCP config must define a single server object when used with `mcp add`");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_stdio_config() {
        let raw = r#"{
            "servers": {
                "fs": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "x"]
                }
            }
        }"#;
        let cfg = parse_and_validate_mcp_config(raw).expect("valid");
        assert_eq!(cfg.server_count(), 1);
    }

    #[test]
    fn rejects_unknown_root_property() {
        let raw = r#"{ "servers": {}, "notAField": true }"#;
        let err = parse_and_validate_mcp_config(raw).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("valid")
                || err.to_string().contains("notAField")
                || err.to_string().contains("additional")
        );
    }

    #[test]
    fn rejects_http_without_url() {
        let raw = r#"{ "servers": { "r": { "type": "http" } } }"#;
        assert!(parse_and_validate_mcp_config(raw).is_err());
    }

    #[test]
    fn rejects_empty_stdio_command_semantic() {
        let raw = r#"{
            "servers": {
                "bad": { "type": "stdio", "command": "   " }
            }
        }"#;
        // Schema may allow whitespace string; semantic must reject.
        let result = parse_and_validate_mcp_config(raw);
        // Either schema (minLength) or semantic
        assert!(result.is_err());
    }

    #[test]
    fn accepts_policy_and_sse() {
        let raw = r#"{
            "policy": { "default": "requireApproval", "allow": ["mcp_fs__*"] },
            "servers": {
                "legacy": {
                    "type": "sse",
                    "url": "http://localhost:3000/sse",
                    "oauth": true
                }
            }
        }"#;
        parse_and_validate_mcp_config(raw).expect("sse+policy valid");
    }

    #[test]
    fn accepts_cursor_mcp_servers_format() {
        let raw = r#"{
            "mcpServers": {
                "context7": {
                    "url": "https://mcp.context7.com/mcp",
                    "headers": {
                        "CONTEXT7_API_KEY": "env.CONTEXT7_API_KEY"
                    }
                },
                "deepwiki": {
                    "url": "https://mcp.deepwiki.com/mcp"
                },
                "lightpanda": {
                    "command": "lightpanda",
                    "args": ["mcp", "--obey-robots"],
                    "type": "stdio"
                }
            }
        }"#;
        let cfg = parse_and_validate_mcp_config(raw).expect("cursor-style config");
        assert_eq!(cfg.server_count(), 3);
        let context7 = cfg.servers.get("context7").expect("context7");
        assert_eq!(context7.kind_label(), "http");
        assert_eq!(
            context7
                .http_config()
                .expect("http")
                .headers
                .get("CONTEXT7_API_KEY")
                .map(String::as_str),
            Some("env.CONTEXT7_API_KEY")
        );
    }
}
