//! MCP server configuration types.
//!
//! Supports:
//! - **stdio** — spawn a local MCP server process
//! - **http** — streamable HTTP MCP endpoint
//! - **sse** — HTTP+SSE MCP endpoint (2024-11-05 protocol)
//! - **policy** — allow / deny / requireApproval for tools
//! - **oauth** — OAuth 2.1 for remote servers (credentials via `mcp auth`)

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::policy::McpPolicyConfig;

/// MCP tool discovery strategy.
///
/// Controls when the client connects to servers and lists their tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpLoadStrategy {
    /// Discover all tools from all enabled servers during registry load.
    #[serde(rename = "eager")]
    Eager,
    /// Defer tool discovery until tools are first requested (e.g. `create_agent_tools` or `call_tool`).
    #[serde(rename = "lazy")]
    #[default]
    Lazy,
}

impl McpLoadStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eager => "eager",
            Self::Lazy => "lazy",
        }
    }
}

fn is_default_lazy(s: &McpLoadStrategy) -> bool {
    *s == McpLoadStrategy::Lazy
}

/// Default per-operation timeout when a server does not override it.
pub const DEFAULT_OPERATION_TIMEOUT_SECS: u64 = 60;

/// Serde default: enabled by default.
fn default_true() -> bool {
    true
}

/// Root MCP configuration.
///
/// Typical locations:
/// - **Home / global:** `~/.elph/mcp.json` (host config dir)
/// - **Project override:** `<project>/.elph/mcp.json` (merged on top of home)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    /// Named server definitions (key is `mcpServers` in JSON).
    #[serde(rename = "mcpServers", default)]
    pub servers: BTreeMap<String, McpServerConfig>,
    /// Global tool policy (merged with per-server `policy`).
    #[serde(default, skip_serializing_if = "McpPolicyConfig::is_empty")]
    pub policy: McpPolicyConfig,
    /// Default tool-result cache TTL in seconds. `0` disables caching. Per-server `cacheTtlMs` wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_secs: Option<u64>,
    /// Max cache entries before eviction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_max_entries: Option<usize>,
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
        self.servers.is_empty()
            && self.policy.is_empty()
            && self.cache_ttl_secs.is_none()
            && self.cache_max_entries.is_none()
    }

    /// Effective cache TTL in seconds (`60` when unset). `0` disables caching.
    pub fn cache_ttl_secs_or_default(&self) -> u64 {
        self.cache_ttl_secs.unwrap_or(60)
    }

    /// Effective cache capacity (`2048` when unset).
    pub fn cache_max_entries_or_default(&self) -> usize {
        self.cache_max_entries.unwrap_or(2048)
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
        if overlay.cache_ttl_secs.is_some() {
            out.cache_ttl_secs = overlay.cache_ttl_secs;
        }
        if overlay.cache_max_entries.is_some() {
            out.cache_max_entries = overlay.cache_max_entries;
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
    /// Remote MCP server over streamable HTTP (`type: "http"`).
    #[serde(rename = "http")]
    Http(McpHttpConfig),
    /// HTTP+SSE MCP transport (`type: "sse"`, 2024-11-05 protocol).
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
            enable: true,
            lifecycle: McpLifecycleMode::Auto,
            mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
            policy: None,
            load_strategy: McpLoadStrategy::default(),
            cache_ttl_ms: None,
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
            Self::Stdio(c) => !c.enable,
            Self::Http(c) | Self::Sse(c) => !c.enable,
        }
    }

    /// The protocol lifecycle mode for this server.
    pub fn lifecycle_mode(&self) -> McpLifecycleMode {
        match self {
            Self::Stdio(c) => c.lifecycle,
            Self::Http(c) | Self::Sse(c) => c.lifecycle,
        }
    }

    /// MRTR elicitation policy for this server.
    pub fn mrtr_elicitation(&self) -> McpMrtrElicitationPolicy {
        match self {
            Self::Stdio(c) => c.mrtr_elicitation,
            Self::Http(c) | Self::Sse(c) => c.mrtr_elicitation,
        }
    }

    /// True when this is the deprecated HTTP+SSE transport (2024-11-05).
    pub fn is_sse(&self) -> bool {
        matches!(self, Self::Sse(_))
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

    /// Tool discovery strategy for this server.
    pub fn load_strategy(&self) -> McpLoadStrategy {
        match self {
            Self::Stdio(c) => c.load_strategy,
            Self::Http(c) | Self::Sse(c) => c.load_strategy,
        }
    }

    /// Tool call result cache TTL in milliseconds. None = use default.
    pub fn cache_ttl_ms(&self) -> Option<u64> {
        match self {
            Self::Stdio(c) => c.cache_ttl_ms,
            Self::Http(c) | Self::Sse(c) => c.cache_ttl_ms,
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
    /// When false, the server is skipped during discovery and tool calls.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Protocol lifecycle mode. `"auto"` probes `server/discover` first and
    /// falls back to `initialize` when unsupported; `"legacy"` uses the old
    /// handshake exclusively; `"discover"` requires 2026-07-28+.
    #[serde(default, skip_serializing_if = "is_default_lifecycle")]
    pub lifecycle: McpLifecycleMode,
    /// MRTR elicitation policy for this server (SEP-2322).
    #[serde(default, skip_serializing_if = "is_default_mrtr_elicitation")]
    pub mrtr_elicitation: McpMrtrElicitationPolicy,
    /// Optional per-server tool policy overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<McpPolicyConfig>,
    /// Tool discovery strategy for this server. Defaults to `lazy`.
    #[serde(default, skip_serializing_if = "is_default_lazy")]
    pub load_strategy: McpLoadStrategy,
    /// Tool call result cache TTL in milliseconds (default 60000).
    /// Set to 0 to disable caching for this server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_ms: Option<u64>,
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

/// Protocol lifecycle mode for MCP connections.
///
/// Controls whether the client uses the legacy `initialize` handshake or the
/// 2026-07-28+ stateless `server/discover` protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpLifecycleMode {
    /// Probe with `server/discover`, falling back to `initialize` when the
    /// server does not support it (non-standard errors such as -32600 are also
    /// treated as "not supported").
    #[serde(rename = "auto")]
    #[default]
    Auto,
    /// Always use the legacy `initialize` / `notifications/initialized`
    /// handshake. Required for servers (e.g. DeepWiki, Context7) that reject
    /// unknown methods with non-standard JSON-RPC error codes.
    #[serde(rename = "legacy")]
    Legacy,
    /// Use `server/discover` exclusively. Fails if the server does not support
    /// the 2026-07-28 protocol.
    #[serde(rename = "discover")]
    Discover,
}

fn is_default_lifecycle(m: &McpLifecycleMode) -> bool {
    *m == McpLifecycleMode::Auto
}

/// How the client answers SEP-2322 MRTR elicitation (`input_required`) rounds.
///
/// Full interactive TUI elicitation is out of scope for the agent tool path;
/// these policies keep tool calls deterministic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpMrtrElicitationPolicy {
    /// Decline elicitation (rmcp default behaviour); tool may continue or fail server-side.
    #[default]
    Decline,
    /// Fail the elicitation request with a clear method error so the agent sees why.
    Error,
}

impl McpMrtrElicitationPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decline => "decline",
            Self::Error => "error",
        }
    }
}

fn is_default_mrtr_elicitation(p: &McpMrtrElicitationPolicy) -> bool {
    *p == McpMrtrElicitationPolicy::Decline
}

/// Client-side list-response cache (SEP-2549 `ttlMs` / `cacheScope`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResponseCacheConfig {
    /// When false, disables rmcp response cache reads/writes.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// TTL used when a server omits `ttlMs` (milliseconds). Default: 0 (immediately stale).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ttl_ms: Option<u64>,
    /// Stable partition key for private-scoped responses (auth principal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_partition: Option<String>,
}

impl Default for McpResponseCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_ttl_ms: None,
            private_partition: None,
        }
    }
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
    /// When false, the server is skipped during discovery and tool calls.
    #[serde(default = "default_true")]
    pub enable: bool,
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
    /// Protocol lifecycle mode. `"auto"` probes `server/discover` first and
    /// falls back to `initialize` when unsupported; `"legacy"` uses the old
    /// handshake exclusively; `"discover"` requires 2026-07-28+.
    #[serde(default, skip_serializing_if = "is_default_lifecycle")]
    pub lifecycle: McpLifecycleMode,
    /// MRTR elicitation policy for this server (SEP-2322).
    #[serde(default, skip_serializing_if = "is_default_mrtr_elicitation")]
    pub mrtr_elicitation: McpMrtrElicitationPolicy,
    /// Optional per-server tool policy overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<McpPolicyConfig>,
    /// Tool discovery strategy for this server. Defaults to `lazy`.
    #[serde(default, skip_serializing_if = "is_default_lazy")]
    pub load_strategy: McpLoadStrategy,
    /// Tool call result cache TTL in milliseconds (default 60000).
    /// Set to 0 to disable caching for this server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_ms: Option<u64>,
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
            enable: true,
            oauth: false,
            oauth_scopes: Vec::new(),
            oauth_client_name: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_client_metadata_url: None,
            oauth_redirect_port: None,
            auth_conflict: McpAuthConflictPolicy::Error,
            lifecycle: McpLifecycleMode::Auto,
            mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
            policy: None,
            load_strategy: McpLoadStrategy::default(),
            cache_ttl_ms: None,
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

/// Per-server MCP discovery progress (for startup UI).
#[derive(Debug, Clone)]
pub enum McpServerLoadProgress {
    Started {
        name: String,
        index: usize,
        total: usize,
    },
    Finished {
        name: String,
        ok: bool,
        transport: String,
        tool_count: usize,
        message: String,
    },
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
    /// Host apps should set this via [`crate::tools::mcp::auth::AuthStorePathBuilder`] /
    /// [`crate::tools::mcp::auth::auth_store_path`] so the library stays path-agnostic
    /// (elph → `~/.elph/auth.json`; other hosts use their own config dirs).
    pub auth_store_path: Option<PathBuf>,
    /// When true (default), also list resources and prompts and expose bridge tools.
    pub discover_resources_and_prompts: bool,
    /// When true (default), listen for tools/list_changed and refresh catalogs.
    pub enable_list_changed: bool,
    /// Client list-response cache (SEP-2549). Applied after each connect.
    pub response_cache: McpResponseCacheConfig,
    /// Optional channel for per-server discovery progress (started / finished).
    pub progress_tx: Option<mpsc::UnboundedSender<McpServerLoadProgress>>,
    /// Tool discovery strategy (default `lazy`).
    pub load_strategy: McpLoadStrategy,
    /// Optional persistent tool call result cache.
    pub cache_store: Option<std::sync::Arc<super::cache::McpCacheStore>>,
    /// Default tool result cache TTL (ms) when a server does not override it.
    /// `0` disables caching. Defaults to 60s.
    pub default_cache_ttl_ms: u64,
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
            response_cache: McpResponseCacheConfig::default(),
            progress_tx: None,
            load_strategy: McpLoadStrategy::default(),
            cache_store: None,
            default_cache_ttl_ms: super::cache::DEFAULT_CACHE_TTL_MS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_stdio_and_http() {
        let json = r#"{
            "mcpServers": {
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
            "mcpServers": {
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
                enable: false,
                lifecycle: McpLifecycleMode::Auto,
                mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
                policy: None,
                load_strategy: McpLoadStrategy::default(),
                cache_ttl_ms: None,
            }),
        );
        assert_eq!(cfg.enabled_count(), 0);
    }

    #[test]
    fn streamable_http_rejected() {
        let json = r#"{"type":"streamableHttp","url":"http://localhost:8080/mcp"}"#;
        let result: Result<McpServerConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "streamableHttp should no longer be accepted");
    }

    #[test]
    fn deserializes_lifecycle_and_mrtr_elicitation() {
        let json = r#"{
            "mcpServers": {
                "modern": {
                    "type": "http",
                    "url": "https://example.com/mcp",
                    "lifecycle": "discover",
                    "mrtrElicitation": "error",
                    "oauthClientMetadataUrl": "https://app.example/oauth-client.json"
                }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(json).expect("parse");
        let server = cfg.servers.get("modern").unwrap();
        assert_eq!(server.lifecycle_mode(), McpLifecycleMode::Discover);
        assert_eq!(server.mrtr_elicitation(), McpMrtrElicitationPolicy::Error);
        assert_eq!(
            server.oauth_meta().unwrap().client_metadata_url.as_deref(),
            Some("https://app.example/oauth-client.json")
        );
    }

    #[test]
    fn response_cache_defaults_enabled() {
        let opts = McpLoadOptions::default();
        assert!(opts.response_cache.enabled);
        assert!(opts.response_cache.default_ttl_ms.is_none());
    }

    #[test]
    fn load_strategy_defaults_lazy() {
        let json = r#"{
            "mcpServers": {
                "fs": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "x"]
                }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(json).expect("parse");
        assert_eq!(cfg.servers.get("fs").unwrap().load_strategy(), McpLoadStrategy::Lazy);
        assert_eq!(McpLoadOptions::default().load_strategy, McpLoadStrategy::Lazy);
    }

    #[test]
    fn load_strategy_eager() {
        let json = r#"{
            "mcpServers": {
                "fs": {
                    "type": "stdio",
                    "command": "npx",
                    "loadStrategy": "eager"
                }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(json).expect("parse");
        assert_eq!(cfg.servers.get("fs").unwrap().load_strategy(), McpLoadStrategy::Eager);
    }

    #[test]
    fn merge_with_project_overrides_server_and_policy() {
        let home: McpConfig = serde_json::from_str(
            r#"{
            "policy": { "default": "requireApproval", "allow": ["mcp_home__*"] },
            "mcpServers": {
                "shared": { "type": "http", "url": "https://home.example/mcp" },
                "home_only": { "type": "stdio", "command": "true" }
            }
        }"#,
        )
        .unwrap();
        let project: McpConfig = serde_json::from_str(
            r#"{
            "policy": { "deny": ["mcp_danger__*"] },
            "mcpServers": {
                "shared": { "type": "http", "url": "https://project.example/mcp", "enable": false },
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
                assert!(!c.enable); // disabled via `enable: false`
            }
            other => panic!("expected http shared, got {other:?}"),
        }
        // Project deny prepended; home allow retained.
        assert!(merged.policy.deny.iter().any(|p| p == "mcp_danger__*"));
        assert!(merged.policy.allow.iter().any(|p| p == "mcp_home__*"));
    }
}
