//! MCP discovery and late binding into an already-running agent session.

use std::path::Path;
use std::sync::Arc;

use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::{McpCacheStore, McpLoadOptions, McpServerLoadProgress, McpToolRegistry};
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

/// Start MCP hot-reload/progress notifications when tools are already on the harness.
pub fn start_mcp_notifications(
    session: &CodingAgentSession,
    mcp_registry: Arc<McpToolRegistry>,
    config_warnings: Vec<String>,
) {
    spawn_mcp_event_loop(session, mcp_registry);
    if !config_warnings.is_empty() {
        let notice = format!(
            "MCP configuration issues (agent started with valid servers only):\n{}",
            config_warnings.join("\n")
        );
        let _ = session.ui_event_sender().send(AgentUiEvent::Status(notice));
    }
}

fn spawn_mcp_event_loop(session: &CodingAgentSession, mcp_registry: Arc<McpToolRegistry>) {
    let harness_for_reload = session.harness();
    let mode_state = session.mode_state();
    let ui_tx = session.ui_event_sender();
    let started = mcp_registry.spawn_event_loop(
        move |registry| {
            let harness = Arc::clone(&harness_for_reload);
            let mode_state = Arc::clone(&mode_state);
            let registry = Arc::clone(&registry);
            tokio::spawn(async move {
                if let Err(error) = apply_mcp_tools_to_harness(&harness, &registry).await {
                    log::warn!("failed to apply MCP hot-reload tools: {error}");
                    return;
                }
                let mode = *mode_state.lock().await;
                if let Err(error) = reconcile_harness_tools(&harness, mode, Some(registry.as_ref())).await {
                    log::warn!("failed to reconcile tools after MCP hot-reload: {error}");
                } else {
                    log::info!("MCP tools hot-reloaded into agent harness");
                }
            });
        },
        move |status| {
            let _ = ui_tx.send(AgentUiEvent::Status(status));
        },
    );
    if started {
        log::info!("MCP event loop (list_changed + progress) started");
    }
}

async fn apply_mcp_tools_to_harness(
    harness: &elph_agent::AgentHarness<elph_agent::TursoSessionStorage>,
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
