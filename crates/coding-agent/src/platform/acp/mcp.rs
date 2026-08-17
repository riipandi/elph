//! Attach client `mcpServers` onto the session MCP registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use agent_client_protocol::schema::v2::McpServer;
use elph_agent::{McpConfig, McpHttpConfig, McpLoadOptions, McpServerConfig, McpToolRegistry};

use crate::agent::CodingAgentSession;
use crate::platform::Paths;
use crate::utils::path::AppPaths;

pub fn map_servers(servers: &[McpServer]) -> Vec<(String, McpServerConfig)> {
    let mut out = Vec::new();
    for server in servers {
        match server {
            McpServer::Stdio(stdio) => {
                let mut cfg = McpServerConfig::stdio(stdio.command.0.display().to_string(), stdio.args.clone());
                if let McpServerConfig::Stdio(stdio_cfg) = &mut cfg {
                    stdio_cfg.env = env_map(&stdio.env);
                }
                out.push((stdio.name.clone(), cfg));
            }
            McpServer::Http(http) => {
                let mut cfg = McpHttpConfig::new(http.url.clone());
                cfg.headers = header_map(&http.headers);
                out.push((http.name.clone(), McpServerConfig::Http(cfg)));
            }
            McpServer::Other(other) if other.type_ == "sse" => {
                if let Some(url) = other.fields.get("url").and_then(|v| v.as_str()) {
                    let mut cfg = McpHttpConfig::new(url);
                    if let Some(headers) = other.fields.get("headers") {
                        cfg.headers = header_map_json(headers);
                    }
                    let name = other
                        .fields
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("sse")
                        .to_string();
                    out.push((name, McpServerConfig::Sse(cfg)));
                }
            }
            McpServer::Other(other) => {
                log::warn!("ignoring unsupported ACP MCP transport '{}'", other.type_);
            }
            _ => {}
        }
    }
    out
}

pub fn map_v1_servers(servers: &[agent_client_protocol::schema::v1::McpServer]) -> Vec<(String, McpServerConfig)> {
    use agent_client_protocol::schema::v1::McpServer;
    let mut out = Vec::new();
    for server in servers {
        match server {
            McpServer::Stdio(stdio) => {
                let mut cfg = McpServerConfig::stdio(stdio.command.display().to_string(), stdio.args.clone());
                if let McpServerConfig::Stdio(stdio_cfg) = &mut cfg {
                    stdio_cfg.env = stdio.env.iter().map(|v| (v.name.clone(), v.value.clone())).collect();
                }
                out.push((stdio.name.clone(), cfg));
            }
            McpServer::Http(http) => {
                let mut cfg = McpHttpConfig::new(http.url.clone());
                cfg.headers = http.headers.iter().map(|h| (h.name.clone(), h.value.clone())).collect();
                out.push((http.name.clone(), McpServerConfig::Http(cfg)));
            }
            McpServer::Sse(sse) => {
                let mut cfg = McpHttpConfig::new(sse.url.clone());
                cfg.headers = sse.headers.iter().map(|h| (h.name.clone(), h.value.clone())).collect();
                out.push((sse.name.clone(), McpServerConfig::Sse(cfg)));
            }
            _ => {}
        }
    }
    out
}

/// Overlay client servers on file MCP config and bind tools into the session.
pub async fn attach_client_servers(
    session: &CodingAgentSession,
    paths: &Paths,
    client_servers: Vec<(String, McpServerConfig)>,
) -> anyhow::Result<usize> {
    let (mut config, warnings) = crate::platform::mcp::load_config_best_effort(paths);
    if client_servers.is_empty() && config.is_empty() {
        return Ok(0);
    }
    for warning in &warnings {
        log::warn!("{warning}");
    }
    let count = client_servers.len();
    for (name, server) in client_servers {
        config.servers.insert(name, server);
    }
    let _ = config;
    let registry = load_registry(paths, config).await;
    session.attach_mcp_registry(registry).await?;
    Ok(count)
}

async fn load_registry(paths: &Paths, config: McpConfig) -> Arc<McpToolRegistry> {
    let load_options = McpLoadOptions {
        auth_store_path: Some(paths.auth_store_path()),
        default_cache_ttl_ms: 60_000,
        ..McpLoadOptions::default()
    };
    match McpToolRegistry::load_with_options(config, load_options).await {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            log::warn!("ACP client MCP attach failed: {error}");
            Arc::new(McpToolRegistry::empty())
        }
    }
}

fn env_map(vars: &[agent_client_protocol::schema::v2::EnvVariable]) -> BTreeMap<String, String> {
    vars.iter().map(|v| (v.name.clone(), v.value.clone())).collect()
}

fn header_map(headers: &[agent_client_protocol::schema::v2::HttpHeader]) -> BTreeMap<String, String> {
    headers.iter().map(|h| (h.name.clone(), h.value.clone())).collect()
}

fn header_map_json(value: &serde_json::Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(arr) = value.as_array() {
        for item in arr {
            let name = item.get("name").and_then(|v| v.as_str());
            let val = item.get("value").and_then(|v| v.as_str());
            if let (Some(name), Some(val)) = (name, val) {
                out.insert(name.to_string(), val.to_string());
            }
        }
    } else if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(val) = v.as_str() {
                out.insert(k.clone(), val.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v2::{EnvVariable, HttpHeader, McpServerHttp, McpServerStdio};
    use std::path::PathBuf;

    #[test]
    fn maps_stdio_env_and_http_headers() {
        let stdio = McpServer::Stdio(
            McpServerStdio::new("local", PathBuf::from("/usr/bin/mcp"))
                .args(vec!["--stdio".into()])
                .env(vec![EnvVariable::new("TOKEN", "secret")]),
        );
        let http = McpServer::Http(
            McpServerHttp::new("remote", "https://mcp.example/mcp").headers(vec![HttpHeader::new("X-Key", "1")]),
        );
        let mapped = map_servers(&[stdio, http]);
        assert_eq!(mapped.len(), 2);
        match &mapped[0].1 {
            McpServerConfig::Stdio(cfg) => {
                assert_eq!(cfg.command, "/usr/bin/mcp");
                assert_eq!(cfg.args, vec!["--stdio"]);
                assert_eq!(cfg.env.get("TOKEN").map(String::as_str), Some("secret"));
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        match &mapped[1].1 {
            McpServerConfig::Http(cfg) => {
                assert_eq!(cfg.url, "https://mcp.example/mcp");
                assert_eq!(cfg.headers.get("X-Key").map(String::as_str), Some("1"));
            }
            other => panic!("expected http, got {other:?}"),
        }
    }
}
