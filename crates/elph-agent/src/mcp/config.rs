//! MCP server configuration types.
//!
//! Supports:
//! - **stdio** — spawn a local MCP server process
//! - **http** / **streamableHttp** — streamable HTTP MCP endpoint
//! - **sse** — legacy HTTP+SSE MCP endpoint
//! - **policy** — allow / deny / requireApproval for tools
//! - **oauth** — OAuth 2.1 for remote servers (credentials via `mcp auth`)

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::policy::McpPolicyConfig;

/// Default per-operation timeout when a server does not override it.
pub const DEFAULT_OPERATION_TIMEOUT_SECS: u64 = 60;

/// Root MCP configuration.
///
/// Typical locations:
/// - **Home / global:** `~/.elph/mcp.json` (host config dir)
/// - **Project override:** `<project>/.elph/mcp.json` (merged on top of home)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    /// Named server definitions.
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerConfig>,
    /// Global tool policy (merged with per-server `policy`).
    #[serde(default, skip_serializing_if = "McpPolicyConfig::is_empty")]
    pub policy: McpPolicyConfig,
}

impl McpConfig {
    /// Servers that are not explicitly disabled.
    pub fn enabled_servers(&self) -> impl Iterator<Item = (&str, &McpServerConfig)> {
        self.servers
            .iter()
            .filter(|(_, cfg)| !cfg.is_disabled())
            .map(|(name, cfg)| (name.as_str(), cfg))
    }

    pub fn is_empty(&self) -> bool {
        self.servers.is_empty() && self.policy.is_empty()
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    pub fn enabled_count(&self) -> usize {
        self.enabled_servers().count()
    }

    /// Effective policy for a server (global + per-server overlay).
    pub fn effective_policy(&self, server: &McpServerConfig) -> McpPolicyConfig {
        match server.policy() {
            Some(server_policy) if !server_policy.is_empty() => self.policy.merge(server_policy),
            _ => self.policy.clone(),
        }
    }

    /// Merge `overlay` on top of this config (project over home).
    ///
    /// - **Servers:** overlay entries replace same-name base entries entirely.
    /// - **Policy:** overlay rules are prepended via [`McpPolicyConfig::merge`].
    pub fn merge_with(&self, overlay: &McpConfig) -> McpConfig {
        let mut out = self.clone();
        if !overlay.policy.is_empty() {
            out.policy = self.policy.merge(&overlay.policy);
        }
        for (name, server) in &overlay.servers {
            out.servers.insert(name.clone(), server.clone());
        }
        out
    }
}

/// One MCP server endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpServerConfig {
    /// Local process speaking MCP over stdio.
    #[serde(rename = "stdio")]
    Stdio(McpStdioConfig),
    /// Remote MCP server over streamable HTTP (`type: "http"` or `"streamableHttp"`).
    #[serde(rename = "http", alias = "streamableHttp", alias = "streamable-http")]
    Http(McpHttpConfig),
    /// Legacy HTTP+SSE transport (`type: "sse"`).
    #[serde(rename = "sse")]
    Sse(McpHttpConfig),
}

impl McpServerConfig {
    pub fn stdio(command: impl Into<String>, args: Vec<String>) -> Self {
        Self::Stdio(McpStdioConfig {
            command: command.into(),
            args,
            env: BTreeMap::new(),
            cwd: None,
            timeout_ms: None,
            disabled: false,
            policy: None,
        })
    }

    pub fn http(url: impl Into<String>) -> Self {
        Self::Http(McpHttpConfig::new(url))
    }

    pub fn sse(url: impl Into<String>) -> Self {
        Self::Sse(McpHttpConfig::new(url))
    }

    /// OAuth client metadata for remote servers (`http` / `sse`).
    pub fn oauth_meta(&self) -> Option<McpOAuthClientMeta> {
        self.http_config().map(|c| c.oauth_meta())
    }

    pub fn is_disabled(&self) -> bool {
        match self {
            Self::Stdio(c) => c.disabled,
            Self::Http(c) | Self::Sse(c) => c.disabled,
        }
    }

    pub fn operation_timeout(&self) -> Duration {
        let ms = match self {
            Self::Stdio(c) => c.timeout_ms,
            Self::Http(c) | Self::Sse(c) => c.timeout_ms,
        };
        Duration::from_millis(ms.unwrap_or(DEFAULT_OPERATION_TIMEOUT_SECS * 1000))
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Stdio(_) => "stdio",
            Self::Http(_) => "http",
            Self::Sse(_) => "sse",
        }
    }

    pub fn policy(&self) -> Option<&McpPolicyConfig> {
        match self {
            Self::Stdio(c) => c.policy.as_ref(),
            Self::Http(c) | Self::Sse(c) => c.policy.as_ref(),
        }
    }

    /// HTTP or SSE URL when this is a remote transport.
    pub fn remote_url(&self) -> Option<&str> {
        match self {
            Self::Http(c) | Self::Sse(c) => Some(c.url.as_str()),
            Self::Stdio(_) => None,
        }
    }

    pub fn http_config(&self) -> Option<&McpHttpConfig> {
        match self {
            Self::Http(c) | Self::Sse(c) => Some(c),
            Self::Stdio(_) => None,
        }
    }

    pub fn wants_oauth(&self) -> bool {
        match self {
            Self::Http(c) | Self::Sse(c) => c.oauth,
            Self::Stdio(_) => false,
        }
    }

    pub fn oauth_scopes(&self) -> &[String] {
        match self {
            Self::Http(c) | Self::Sse(c) => &c.oauth_scopes,
            Self::Stdio(_) => &[],
        }
    }
}

/// Stdio (child process) server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpStdioConfig {
    /// Executable to spawn (absolute path or PATH lookup).
    pub command: String,
    /// Command arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables for the child process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional working directory for the child process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Per-operation timeout in milliseconds (list tools, call tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// When true, the server is skipped during discovery and tool calls.
    #[serde(default)]
    pub disabled: bool,
    /// Optional per-server tool policy overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<McpPolicyConfig>,
}

/// OAuth client metadata used by `mcp auth` and token-aware transports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpOAuthClientMeta {
    pub scopes: Vec<String>,
    pub client_name: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_metadata_url: Option<String>,
    pub redirect_port: Option<u16>,
}

/// How to resolve competing credentials (env/inline bearer vs `auth.json` OAuth).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpAuthConflictPolicy {
    /// Fail if both static bearer (authToken / authTokenEnv) and auth.json OAuth exist.
    #[default]
    Error,
    /// Prefer authToken / authTokenEnv over auth.json (CI overrides).
    PreferEnv,
    /// Prefer OAuth tokens in auth.json over static bearer.
    PreferOauth,
}

impl McpAuthConflictPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::PreferEnv => "preferEnv",
            Self::PreferOauth => "preferOauth",
        }
    }
}

/// Streamable HTTP or legacy SSE MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpConfig {
    /// Base URL of the MCP endpoint (e.g. `https://host/mcp` or `http://host/sse`).
    pub url: String,
    /// Extra HTTP headers sent with every request.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Bearer token value (without `Bearer ` prefix). Prefer `auth_token_env` for secrets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// Environment variable name holding a bearer token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token_env: Option<String>,
    /// Per-operation timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// When true, the server is skipped during discovery and tool calls.
    #[serde(default)]
    pub disabled: bool,
    /// Prefer OAuth credentials from `mcp auth` (and run OAuth-aware transport).
    #[serde(default)]
    pub oauth: bool,
    /// Optional OAuth scopes requested during `mcp auth`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_scopes: Vec<String>,
    /// OAuth client display name (dynamic registration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_name: Option<String>,
    /// Pre-registered OAuth client id (skips dynamic registration when set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_id: Option<String>,
    /// Optional client secret for confidential clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_secret: Option<String>,
    /// SEP-991 URL-based client id / client metadata document URL (https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_metadata_url: Option<String>,
    /// Fixed loopback redirect port for OAuth (default: ephemeral OS port).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_redirect_port: Option<u16>,
    /// Conflict policy when both static bearer and auth.json OAuth are present.
    #[serde(default, skip_serializing_if = "is_default_auth_conflict")]
    pub auth_conflict: McpAuthConflictPolicy,
    /// Optional per-server tool policy overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<McpPolicyConfig>,
}

fn is_default_auth_conflict(p: &McpAuthConflictPolicy) -> bool {
    *p == McpAuthConflictPolicy::Error
}

impl McpHttpConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: BTreeMap::new(),
            auth_token: None,
            auth_token_env: None,
            timeout_ms: None,
            disabled: false,
            oauth: false,
            oauth_scopes: Vec::new(),
            oauth_client_name: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_client_metadata_url: None,
            oauth_redirect_port: None,
            auth_conflict: McpAuthConflictPolicy::Error,
            policy: None,
        }
    }

    pub fn oauth_meta(&self) -> McpOAuthClientMeta {
        McpOAuthClientMeta {
            scopes: self.oauth_scopes.clone(),
            client_name: self.oauth_client_name.clone(),
            client_id: self.oauth_client_id.clone(),
            client_secret: self.oauth_client_secret.clone(),
            client_metadata_url: self.oauth_client_metadata_url.clone(),
            redirect_port: self.oauth_redirect_port,
        }
    }

    /// Resolve bearer token from inline value or environment.
    pub fn resolve_auth_token(&self) -> Option<String> {
        if let Some(token) = &self.auth_token
            && !token.is_empty()
        {
            return Some(token.clone());
        }
        self.auth_token_env
            .as_ref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|v| !v.is_empty())
    }
}

/// Options controlling how a registry discovers tools.
#[derive(Debug, Clone)]
pub struct McpLoadOptions {
    /// When true (default), a failing server is recorded and skipped instead of failing the whole load.
    pub continue_on_error: bool,
    /// Max concurrent server discovery tasks (default 4).
    pub max_concurrency: usize,
    /// Override global discovery timeout per server (defaults to each server's timeout).
    pub discovery_timeout: Option<Duration>,
    /// Full path to the shared OAuth credential store file (default name `auth.json`).
    ///
    /// Host apps should set this via [`crate::mcp::auth::AuthStorePathBuilder`] /
    /// [`crate::mcp::auth::auth_store_path`] so the library stays path-agnostic
    /// (elph → `~/.elph/auth.json`; other hosts use their own config dirs).
    pub auth_store_path: Option<PathBuf>,
    /// When true (default), also list resources and prompts and expose bridge tools.
    pub discover_resources_and_prompts: bool,
    /// When true (default), listen for tools/list_changed and refresh catalogs.
    pub enable_list_changed: bool,
}

impl Default for McpLoadOptions {
    fn default() -> Self {
        Self {
            continue_on_error: true,
            max_concurrency: 4,
            discovery_timeout: None,
            auth_store_path: None,
            discover_resources_and_prompts: true,
            enable_list_changed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_stdio_and_http() {
        let json = r#"{
            "servers": {
                "local": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                },
                "remote": {
                    "type": "http",
                    "url": "https://example.com/mcp",
                    "authTokenEnv": "MCP_TOKEN"
                }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(json).expect("parse");
        assert_eq!(cfg.server_count(), 2);
        assert_eq!(cfg.enabled_count(), 2);
        assert!(matches!(cfg.servers.get("local"), Some(McpServerConfig::Stdio(_))));
        assert!(matches!(cfg.servers.get("remote"), Some(McpServerConfig::Http(_))));
    }

    #[test]
    fn deserializes_sse_and_policy() {
        let json = r#"{
            "policy": { "default": "requireApproval", "allow": ["mcp_fs__*"] },
            "servers": {
                "legacy": {
                    "type": "sse",
                    "url": "http://localhost:3000/sse",
                    "oauth": true,
                    "oauthScopes": ["read"]
                }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(json).expect("parse");
        assert!(matches!(cfg.servers.get("legacy"), Some(McpServerConfig::Sse(_))));
        assert!(cfg.servers.get("legacy").unwrap().wants_oauth());
        assert_eq!(cfg.policy.allow, vec!["mcp_fs__*".to_string()]);
    }

    #[test]
    fn disabled_servers_filtered() {
        let mut cfg = McpConfig::default();
        cfg.servers.insert(
            "a".into(),
            McpServerConfig::Stdio(McpStdioConfig {
                command: "true".into(),
                args: vec![],
                env: BTreeMap::new(),
                cwd: None,
                timeout_ms: None,
                disabled: true,
                policy: None,
            }),
        );
        assert_eq!(cfg.enabled_count(), 0);
    }

    #[test]
    fn streamable_http_alias() {
        let json = r#"{"type":"streamableHttp","url":"http://localhost:8080/mcp"}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).expect("parse");
        assert!(matches!(cfg, McpServerConfig::Http(_)));
    }

    #[test]
    fn merge_with_project_overrides_server_and_policy() {
        let home: McpConfig = serde_json::from_str(
            r#"{
            "policy": { "default": "requireApproval", "allow": ["mcp_home__*"] },
            "servers": {
                "shared": { "type": "http", "url": "https://home.example/mcp" },
                "home_only": { "type": "stdio", "command": "true" }
            }
        }"#,
        )
        .unwrap();
        let project: McpConfig = serde_json::from_str(
            r#"{
            "policy": { "deny": ["mcp_danger__*"] },
            "servers": {
                "shared": { "type": "http", "url": "https://project.example/mcp", "disabled": true },
                "project_only": { "type": "stdio", "command": "npx" }
            }
        }"#,
        )
        .unwrap();

        let merged = home.merge_with(&project);
        assert_eq!(merged.server_count(), 3);
        assert!(merged.servers.contains_key("home_only"));
        assert!(merged.servers.contains_key("project_only"));
        match merged.servers.get("shared") {
            Some(McpServerConfig::Http(c)) => {
                assert_eq!(c.url, "https://project.example/mcp");
                assert!(c.disabled);
            }
            other => panic!("expected http shared, got {other:?}"),
        }
        // Project deny prepended; home allow retained.
        assert!(merged.policy.deny.iter().any(|p| p == "mcp_danger__*"));
        assert!(merged.policy.allow.iter().any(|p| p == "mcp_home__*"));
    }
}
