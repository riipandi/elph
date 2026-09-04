//! MCP discovery and late binding into an already-running agent session.

use std::hash::Hash;
use std::hash::Hasher;
use std::sync::{Arc, Weak};

use crate::types::AgentMode;
use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::harness::AgentHarness;
use elph_agent::mcp::{McpCacheStore, McpConfig, McpLoadOptions, McpServerLoadProgress, McpToolRegistry};
use elph_agent::session::TursoSessionStorage;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use super::events::AgentUiEvent;
use super::session::CodingAgentSession;
use super::tools_catalog::reconcile_harness_tools;
use crate::platform::Paths;

/// Load merged MCP config and discover remote tool catalogs.
pub async fn discover_mcp_registry(
    paths: &Paths,
    cache_store: Option<Arc<McpCacheStore>>,
    default_cache_ttl_ms: u64,
) -> (Arc<McpToolRegistry>, Vec<String>) {
    discover_mcp_registry_with_progress(paths, None, cache_store, default_cache_ttl_ms).await
}

/// Like [`discover_mcp_registry`], emitting per-server progress events when `progress_tx` is set.
pub async fn discover_mcp_registry_with_progress(
    paths: &Paths,
    progress_tx: Option<mpsc::UnboundedSender<McpServerLoadProgress>>,
    cache_store: Option<Arc<McpCacheStore>>,
    default_cache_ttl_ms: u64,
) -> (Arc<McpToolRegistry>, Vec<String>) {
    let (mcp_config, mcp_config_warnings) = crate::platform::mcp::load_config_best_effort(paths);
    for warning in &mcp_config_warnings {
        log::warn!("{warning}");
    }
    let auth_store_path = paths.auth_store_path();
    let load_options = McpLoadOptions {
        auth_store_path: Some(auth_store_path),
        progress_tx,
        cache_store,
        default_cache_ttl_ms,
        ..McpLoadOptions::default()
    };
    let registry = match McpToolRegistry::load_with_options(mcp_config, load_options).await {
        Ok(registry) => {
            let report = registry.load_report();
            if report.servers_failed > 0 {
                log::warn!(
                    "MCP discovery finished with server failures: ok={} failed={} tools={}",
                    report.servers_ok,
                    report.servers_failed,
                    report.tools_loaded
                );
                for server in &report.servers {
                    if !server.ok {
                        log::warn!("MCP server unavailable: server={} error={}", server.name, server.message);
                    }
                }
            }
            Arc::new(registry)
        }
        Err(error) => {
            log::warn!("MCP tool discovery failed: {error}");
            Arc::new(McpToolRegistry::empty())
        }
    };
    (registry, mcp_config_warnings)
}

/// Dynamic hot-reload target for a shared MCP registry.
///
/// The MCP event loop can outlive individual sessions when the registry is
/// shared across in-process session reloads (`/new`, `/resume`); it reads this
/// target per event so hot reloads always apply to the currently live session.
#[derive(Clone)]
pub struct McpReloadTarget {
    harness: Weak<AgentHarness<TursoSessionStorage>>,
    mode_state: Weak<Mutex<AgentMode>>,
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
}

/// Registry shared across in-process session reloads, plus the fingerprint of
/// the MCP config it was built from and the current hot-reload target.
///
/// Sharing keeps the session pool (stdio processes / HTTP sessions) and the
/// discovered catalog alive across `/new` instead of reconnecting every server
/// per conversation. A changed config fingerprint rebuilds the registry.
pub struct SharedMcp {
    registry: Arc<McpToolRegistry>,
    fingerprint: u64,
    target: RwLock<Option<McpReloadTarget>>,
}

impl Clone for SharedMcp {
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            fingerprint: self.fingerprint,
            target: RwLock::new(self.target.read().clone()),
        }
    }
}

impl std::fmt::Debug for SharedMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMcp")
            .field("fingerprint", &self.fingerprint)
            .field("target_bound", &self.target.read().is_some())
            .finish_non_exhaustive()
    }
}

impl SharedMcp {
    pub fn from_registry(registry: Arc<McpToolRegistry>) -> Self {
        let fingerprint = fingerprint_mcp_config(registry.config());
        Self {
            registry,
            fingerprint,
            target: RwLock::new(None),
        }
    }

    pub fn registry(&self) -> &Arc<McpToolRegistry> {
        &self.registry
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Point hot reloads at this session's harness / mode / UI channel.
    fn bind(&self, session: &CodingAgentSession) {
        *self.target.write() = Some(McpReloadTarget {
            harness: Arc::downgrade(&session.harness()),
            mode_state: Arc::downgrade(&session.mode_state()),
            ui_tx: session.ui_event_sender(),
        });
    }
}

/// Stable fingerprint of an MCP config (servers + policy). Two configs with the
/// same fingerprint can safely share one registry / pool.
pub fn fingerprint_mcp_config(config: &McpConfig) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match serde_json::to_vec(config) {
        Ok(bytes) => bytes.hash(&mut hasher),
        Err(error) => {
            log::debug!("MCP config fingerprint serialization failed: {error:#}");
            config.enabled_servers().count().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Reuse the previous shared registry when the MCP config is unchanged; build a
/// fresh one otherwise (deferred discovery, matching the TUI bootstrap path).
pub async fn load_shared_mcp(paths: &Paths, previous: Option<&SharedMcp>) -> SharedMcp {
    let (mcp_config, _warnings) = crate::platform::mcp::load_config_best_effort(paths);
    let fingerprint = fingerprint_mcp_config(&mcp_config);
    if let Some(previous) = previous
        && previous.fingerprint == fingerprint
    {
        return previous.clone();
    }
    let load_options = McpLoadOptions {
        auth_store_path: Some(paths.auth_store_path()),
        skip_startup_discovery: true,
        ..McpLoadOptions::default()
    };
    let registry = match McpToolRegistry::load_with_options(mcp_config, load_options).await {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            log::warn!("shared MCP registry load failed: {error:#}");
            Arc::new(McpToolRegistry::empty())
        }
    };
    SharedMcp::from_registry(registry)
}

/// Start MCP hot-reload/progress notifications when tools are already on the harness.
pub fn start_mcp_notifications(session: &CodingAgentSession, shared: &Arc<SharedMcp>, config_warnings: Vec<String>) {
    shared.bind(session);
    spawn_mcp_event_loop(shared);
    if !config_warnings.is_empty() {
        let notice = format!(
            "MCP configuration issues (agent started with valid servers only):\n{}",
            config_warnings.join("\n")
        );
        let _ = session.ui_event_sender().send(AgentUiEvent::Status(notice));
    }
}

fn spawn_mcp_event_loop(shared: &Arc<SharedMcp>) {
    // Hold the shared state weakly: this task lives as long as the registry's
    // event channel (the registry pool owns the sender), and the harness's MCP
    // tools own strong `Arc<McpToolRegistry>` clones. A strong capture here
    // forms loop → shared → registry → sender, which keeps `rx.recv()` pending
    // forever and prevents the loop from ever observing channel closure.
    let shared_for_refresh = Arc::downgrade(shared);
    let shared_for_progress = Arc::downgrade(shared);
    let started = shared.registry().spawn_event_loop(
        move |registry| {
            // Shared state dropped — no live session left to reload into.
            let Some(shared) = shared_for_refresh.upgrade() else {
                return;
            };
            let Some(target) = shared.target.read().clone() else {
                return;
            };
            let Some(harness) = target.harness.upgrade() else {
                return;
            };
            let registry = Arc::clone(&registry);
            tokio::spawn(async move {
                if let Err(error) = apply_mcp_tools_to_harness(&harness, &registry).await {
                    log::warn!("failed to apply MCP hot-reload tools: {error}");
                    return;
                }
                let Some(mode_state) = target.mode_state.upgrade() else {
                    return;
                };
                let mode = *mode_state.lock().await;
                if let Err(error) = reconcile_harness_tools(&harness, mode, Some(registry.as_ref()), None).await {
                    log::warn!("failed to reconcile tools after MCP hot-reload: {error}");
                } else {
                    log::info!("MCP tools hot-reloaded into agent harness");
                }
            });
        },
        move |status| {
            if let Some(shared) = shared_for_progress.upgrade()
                && let Some(target) = shared.target.read().clone()
            {
                let _ = target.ui_tx.send(AgentUiEvent::Status(status));
            }
        },
    );
    if started {
        log::info!("MCP event loop (list_changed + progress) started");
    }
}

async fn apply_mcp_tools_to_harness(
    harness: &elph_agent::harness::AgentHarness<elph_agent::session::TursoSessionStorage>,
    registry: &Arc<McpToolRegistry>,
) -> Result<()> {
    let mcp_tools = registry.create_agent_tools().await;
    let mut kept: Vec<_> = harness
        .get_tools()
        .await
        .into_iter()
        .filter(|t| !t.name().starts_with("mcp_"))
        .collect();
    kept.extend(mcp_tools);

    harness
        .set_tools(kept, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::mcp::McpServerConfig;

    #[test]
    fn fingerprint_is_stable_and_config_sensitive() {
        let mut config = McpConfig::default();
        config
            .servers
            .insert("alpha".into(), McpServerConfig::stdio("uvx", vec!["mcp-alpha".into()]));

        let cloned = config.clone();
        assert_eq!(fingerprint_mcp_config(&config), fingerprint_mcp_config(&cloned));

        let mut changed = config.clone();
        changed
            .servers
            .insert("beta".into(), McpServerConfig::stdio("uvx", vec!["mcp-beta".into()]));
        assert_ne!(fingerprint_mcp_config(&config), fingerprint_mcp_config(&changed));

        let empty = McpConfig::default();
        assert_ne!(fingerprint_mcp_config(&config), fingerprint_mcp_config(&empty));
    }
}
