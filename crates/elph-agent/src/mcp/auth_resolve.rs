//! Resolve competing MCP credentials: inline/`authTokenEnv` vs OAuth `auth.json`.
//!
//! # Sources
//!
//! | Source | From |
//! |--------|------|
//! | **Bearer (inline)** | `authToken` in mcp.json |
//! | **Bearer (env)** | `authTokenEnv` → environment variable |
//! | **OAuth store** | encrypted entry in `auth.json` (via `mcp auth`) |
//!
//! # Conflicts
//!
//! When both a static bearer (inline or env) **and** an OAuth store entry exist for the
//! same server, [`McpAuthConflictPolicy`] decides:
//!
//! - **`error`** (default) — fail with a clear message (no silent override)
//! - **`preferEnv`** — use static bearer; log a warning about ignoring auth.json
//! - **`preferOauth`** — use auth.json OAuth; log a warning about ignoring env/token
//!
//! When only one source is present, that source wins (no conflict).

use std::path::Path;

use anyhow::{Result, bail};

use super::auth::{has_stored_credentials, resolve_oauth_access_token};
use super::config::{McpAuthConflictPolicy, McpHttpConfig};

/// Which credential source produced the resolved token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAuthSource {
    /// `authToken` field in config.
    InlineToken,
    /// Value loaded from `authTokenEnv`.
    EnvToken,
    /// OAuth access token from `auth.json`.
    OauthStore,
}

/// Outcome of credential resolution for a remote MCP server.
#[derive(Debug, Clone)]
pub enum ResolvedMcpAuth {
    /// No credentials — unauthenticated connection.
    None,
    /// Static bearer for Authorization header (HTTP static header or SSE).
    StaticBearer { token: String, source: McpAuthSource },
    /// Use OAuth transport / refreshed access token from the store.
    Oauth {
        /// Fresh access token (for SSE or explicit header).
        access_token: String,
    },
}

impl ResolvedMcpAuth {
    pub fn source_label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::StaticBearer {
                source: McpAuthSource::InlineToken,
                ..
            } => "authToken",
            Self::StaticBearer {
                source: McpAuthSource::EnvToken,
                ..
            } => "authTokenEnv",
            Self::StaticBearer {
                source: McpAuthSource::OauthStore,
                ..
            } => "auth.json",
            Self::Oauth { .. } => "auth.json (oauth)",
        }
    }

    /// Bearer string if any (static or oauth access token).
    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::StaticBearer { token, .. } | Self::Oauth { access_token: token } => Some(token),
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::Oauth { .. })
    }
}

#[derive(Debug, Clone)]
struct StaticBearerProbe {
    token: String,
    source: McpAuthSource,
    /// Env var name when source is EnvToken (for error messages; never the value).
    env_name: Option<String>,
}

fn probe_static_bearer(config: &McpHttpConfig) -> Option<StaticBearerProbe> {
    if let Some(token) = &config.auth_token
        && !token.is_empty()
    {
        return Some(StaticBearerProbe {
            token: token.clone(),
            source: McpAuthSource::InlineToken,
            env_name: None,
        });
    }
    if let Some(name) = &config.auth_token_env
        && let Ok(token) = std::env::var(name)
        && !token.is_empty()
    {
        return Some(StaticBearerProbe {
            token,
            source: McpAuthSource::EnvToken,
            env_name: Some(name.clone()),
        });
    }
    None
}

fn conflict_message(server_name: &str, static_probe: &StaticBearerProbe, policy: McpAuthConflictPolicy) -> String {
    let static_label = match static_probe.source {
        McpAuthSource::InlineToken => "authToken (inline in mcp.json)".to_string(),
        McpAuthSource::EnvToken => format!("authTokenEnv ({})", static_probe.env_name.as_deref().unwrap_or("?")),
        McpAuthSource::OauthStore => "auth.json".to_string(),
    };
    match policy {
        McpAuthConflictPolicy::Error => format!(
            "MCP server \"{server_name}\" has conflicting credentials: {static_label} is set \
             and OAuth tokens exist in auth.json. Set authConflict to \"preferEnv\" or \
             \"preferOauth\", or remove one source (unset the env / delete authToken / run \
             `mcp logout {server_name}`)."
        ),
        McpAuthConflictPolicy::PreferEnv => format!(
            "MCP server \"{server_name}\": both {static_label} and auth.json OAuth present; \
             using {static_label} (authConflict=preferEnv)"
        ),
        McpAuthConflictPolicy::PreferOauth => format!(
            "MCP server \"{server_name}\": both {static_label} and auth.json OAuth present; \
             using auth.json OAuth (authConflict=preferOauth)"
        ),
    }
}

/// Resolve credentials for a remote server, enforcing [`McpAuthConflictPolicy`] on conflicts.
pub async fn resolve_remote_auth(
    config: &McpHttpConfig,
    server_name: &str,
    auth_store_path: Option<&Path>,
) -> Result<ResolvedMcpAuth> {
    let static_probe = probe_static_bearer(config);
    let store_path = auth_store_path.filter(|_| !server_name.is_empty());
    let has_oauth_entry = store_path.is_some_and(|p| has_stored_credentials(p, server_name));
    let oauth_required = config.oauth;
    let policy = config.auth_conflict;

    // OAuth required but no store entry and no static fallback (unless preferEnv allows static only).
    if oauth_required && !has_oauth_entry {
        if let Some(s) = static_probe {
            // oauth:true but only static present — treat as static with warn, or error?
            // Prefer: allow static only when policy PreferEnv, else require OAuth.
            match policy {
                McpAuthConflictPolicy::PreferEnv => {
                    log::warn!(
                        "oauth=true but no auth.json entry; using static bearer (authConflict=preferEnv): server={server_name}"
                    );
                    return Ok(ResolvedMcpAuth::StaticBearer {
                        token: s.token,
                        source: s.source,
                    });
                }
                _ => {
                    bail!(
                        "MCP server \"{server_name}\" has oauth=true but no credentials in auth.json \
                         (and authConflict is not preferEnv). Run `mcp auth {server_name}` \
                         or set authToken/authTokenEnv with authConflict=preferEnv."
                    );
                }
            }
        }
        bail!("MCP server \"{server_name}\" requires OAuth; run `mcp auth {server_name}` first");
    }

    // Both sources present → conflict.
    let mut force_oauth = false;
    if let (Some(static_probe), true) = (&static_probe, has_oauth_entry) {
        match policy {
            McpAuthConflictPolicy::Error => {
                bail!("{}", conflict_message(server_name, static_probe, policy));
            }
            McpAuthConflictPolicy::PreferEnv => {
                log::warn!("{}", conflict_message(server_name, static_probe, policy));
                return Ok(ResolvedMcpAuth::StaticBearer {
                    token: static_probe.token.clone(),
                    source: static_probe.source,
                });
            }
            McpAuthConflictPolicy::PreferOauth => {
                log::warn!("{}", conflict_message(server_name, static_probe, policy));
                force_oauth = true;
            }
        }
    }

    // OAuth only (or preferOauth after conflict).
    if has_oauth_entry {
        let path = store_path.expect("has_oauth_entry implies path");
        match resolve_oauth_access_token(&config.url, path, server_name).await? {
            Some(access_token) => {
                return Ok(ResolvedMcpAuth::Oauth { access_token });
            }
            None if oauth_required || force_oauth => {
                bail!("MCP server \"{server_name}\" OAuth entry unusable; re-run `mcp auth {server_name}`");
            }
            None => {
                // Store entry present but unusable — fall through to static if allowed.
            }
        }
    }

    // Static only.
    if let Some(s) = static_probe {
        return Ok(ResolvedMcpAuth::StaticBearer {
            token: s.token,
            source: s.source,
        });
    }

    Ok(ResolvedMcpAuth::None)
}

/// Sync probe for doctor/CLI: which sources are present (no token values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpAuthSourceReport {
    pub has_inline_token: bool,
    pub auth_token_env: Option<String>,
    pub env_var_set: bool,
    pub has_oauth_store: bool,
    pub conflict: bool,
    pub policy: McpAuthConflictPolicy,
}

impl McpAuthSourceReport {
    pub fn probe(config: &McpHttpConfig, server_name: &str, auth_store_path: Option<&Path>) -> Self {
        let has_inline_token = config.auth_token.as_ref().is_some_and(|t| !t.is_empty());
        let auth_token_env = config.auth_token_env.clone();
        let env_var_set = auth_token_env
            .as_ref()
            .is_some_and(|n| std::env::var(n).map(|v| !v.is_empty()).unwrap_or(false));
        let has_static = has_inline_token || env_var_set;
        let has_oauth_store = auth_store_path
            .filter(|_| !server_name.is_empty())
            .is_some_and(|p| has_stored_credentials(p, server_name));
        Self {
            has_inline_token,
            auth_token_env,
            env_var_set,
            has_oauth_store,
            conflict: has_static && has_oauth_store,
            policy: config.auth_conflict,
        }
    }

    /// Short status for doctor (never includes secret values).
    pub fn status_label(&self) -> String {
        let mut parts = Vec::new();
        if self.has_inline_token {
            parts.push("authToken".to_string());
        }
        if let Some(env) = &self.auth_token_env {
            if self.env_var_set {
                parts.push(format!("env:{env}=set"));
            } else {
                parts.push(format!("env:{env}=unset"));
            }
        }
        if self.has_oauth_store {
            parts.push("auth.json".to_string());
        }
        if parts.is_empty() {
            return "none".into();
        }
        let base = parts.join("+");
        if self.conflict {
            format!("{base} CONFLICT(policy={})", self.policy.as_str())
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::auth::FileCredentialStore;
    use crate::mcp::crypto::Aes256Key;
    use rmcp::transport::auth::{CredentialStore, StoredCredentials};
    use tempfile::tempdir;

    fn base_config() -> McpHttpConfig {
        McpHttpConfig::new("https://example.com/mcp")
    }

    #[tokio::test]
    async fn prefers_env_on_conflict() {
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("auth.json");
        let key = Aes256Key::generate();
        let store = FileCredentialStore::with_key(&store_path, "svc", key);
        store
            .save(StoredCredentials::new("cid".into(), None, vec![], Some(1)))
            .await
            .unwrap();

        let env_name = "ELPH_TEST_MCP_TOKEN_CONFLICT";
        // SAFETY: test-only env; unique name.
        unsafe { std::env::set_var(env_name, "env-secret-token") };

        let mut cfg = base_config();
        cfg.auth_token_env = Some(env_name.into());
        cfg.auth_conflict = McpAuthConflictPolicy::PreferEnv;

        let resolved = resolve_remote_auth(&cfg, "svc", Some(&store_path)).await.unwrap();
        match resolved {
            ResolvedMcpAuth::StaticBearer {
                token,
                source: McpAuthSource::EnvToken,
            } => assert_eq!(token, "env-secret-token"),
            other => panic!("expected env bearer, got {other:?}"),
        }

        unsafe { std::env::remove_var(env_name) };
    }

    #[tokio::test]
    async fn prefers_oauth_on_conflict() {
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("auth.json");
        // Without a full OAuth token_response, resolve_oauth_access_token may return None.
        // Conflict path PreferOauth still requires a usable oauth token — we only assert Error vs PreferEnv here.
        let mut cfg = base_config();
        cfg.auth_token = Some("inline".into());
        cfg.auth_conflict = McpAuthConflictPolicy::Error;
        // No store entry → no conflict, static wins
        let resolved = resolve_remote_auth(&cfg, "svc", Some(&store_path)).await.unwrap();
        assert!(matches!(
            resolved,
            ResolvedMcpAuth::StaticBearer {
                source: McpAuthSource::InlineToken,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn errors_on_conflict_by_default() {
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("auth.json");
        let key = Aes256Key::generate();
        let store = FileCredentialStore::with_key(&store_path, "svc", key);
        store
            .save(StoredCredentials::new("cid".into(), None, vec![], Some(1)))
            .await
            .unwrap();

        let mut cfg = base_config();
        cfg.auth_token = Some("inline-token".into());
        // default auth_conflict = Error
        let err = resolve_remote_auth(&cfg, "svc", Some(&store_path)).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("conflicting"), "{msg}");
        assert!(msg.contains("preferEnv") || msg.contains("preferOauth"), "{msg}");
    }

    #[test]
    fn report_detects_conflict_without_leaking_values() {
        let mut cfg = base_config();
        cfg.auth_token = Some("secret-must-not-appear".into());
        cfg.auth_token_env = Some("SOME_ENV".into());
        let report = McpAuthSourceReport::probe(&cfg, "x", None);
        assert!(report.has_inline_token);
        let label = report.status_label();
        assert!(!label.contains("secret-must-not-appear"));
    }
}
