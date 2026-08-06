//! MCP tool registry — discover remote tools/resources/prompts and bridge them into the agent harness.
//!
//! Production path:
//! 1. [`McpToolRegistry::load`] / [`load_with_options`] validates config and optionally discovers catalogs.
//! 2. Sessions are pooled so tool calls reuse stdio processes / HTTP sessions.
//! 3. Lazy discovery: [`McpToolRegistry::ensure_server_discovered`] fires discovery exactly once per
//!    server (with 1 retry on transient failure) on first `call_tool`/`read_resource`/`get_prompt`.
//!    Results are merged into the catalog — other servers' tools are preserved.
//! 4. [`McpToolRegistry::create_agent_tools`] exposes `mcp_{server}__{tool}` agent tools. On failure,
//!    already-discovered tools are still returned (graceful degradation).
//! 5. Policy filters deny-listed tools; approval is enforced via [`crate::tools::mcp::policy`].
//! 6. `tools/list_changed` (and resource/prompt variants) can refresh catalogs in place.
//! 7. The TUI bootstrap (`bootstrap_mcp_for_session`) always attaches tools even when discovery
//!    has partial failures — partial results are better than no tools.
//!
//! ## Call flow (fix for "Requires HTTP transport")
//!
//! `call_tool_on_client` uses `call_tool_once` (non-MRTR) instead of the MRTR-aware `call_tool`.
//! The MRTR variant requires HTTP transport and fails on stdio with `"Requires HTTP transport (--port)"`.
//! Since the agent harness does not support interactive MRTR rounds (policy: `Decline`), `call_tool_once`
//! is the correct choice for all transports. Task handles (SEP-2663) are still surfaced via `tasks_get`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::stream::StreamExt;
use futures::stream::{self};
use parking_lot::RwLock;
use rmcp::model::{CallToolResult, ContentBlock, ResourceContents};
use serde_json::Value;
use serde_json::json;
use tokio::sync::mpsc;

use crate::types::{AgentToolResult, ToolResultContent};

use super::client::{call_tool_for_server, probe_server_with_auth};
use super::config::{McpConfig, McpLoadOptions, McpLoadStrategy, McpServerConfig, McpServerLoadProgress};
use super::events::McpServerEvent;
use super::policy::McpPolicyConfig;
use super::policy::mcp_tool_requires_approval;
use super::session::McpSessionPool;

use discovery::{ServerDiscovery, build_catalogs_from_results, discover_one, server_discovery_progress};
use super::truncate::{DEFAULT_MAX_STRUCTURED_DETAIL_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS};
use super::truncate::{truncate_json_value, truncate_tool_content};


mod discovery;
mod bridge;

/// A discovered MCP tool ready for exposure to the model.
#[derive(Debug, Clone)]
pub struct McpToolDescriptor {
    pub server_name: String,
    pub tool_name: String,
    pub exposed_name: String,
    pub description: String,
    pub parameters: Value,
    pub requires_approval: bool,
}

/// Discovered resource metadata.
#[derive(Debug, Clone)]
pub struct McpResourceDescriptor {
    pub server_name: String,
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
}

/// Discovered prompt metadata.
#[derive(Debug, Clone)]
pub struct McpPromptDescriptor {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub arguments_schema: Value,
}

/// Per-server discovery outcome.
#[derive(Debug, Clone)]
pub struct McpServerLoadReport {
    pub name: String,
    pub ok: bool,
    pub transport: String,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub message: String,
}

/// Full registry load report (for doctor / logging).
#[derive(Debug, Clone, Default)]
pub struct McpLoadReport {
    pub servers: Vec<McpServerLoadReport>,
    pub tools_loaded: usize,
    pub resources_loaded: usize,
    pub prompts_loaded: usize,
    pub servers_ok: usize,
    pub servers_failed: usize,
    pub servers_skipped: usize,
}

fn should_discover_server(global_strategy: McpLoadStrategy, server_strategy: McpLoadStrategy) -> bool {
    global_strategy == McpLoadStrategy::Eager || server_strategy == McpLoadStrategy::Eager
}

fn should_auto_discover_on_startup(global_strategy: McpLoadStrategy, server_strategy: McpLoadStrategy) -> bool {
    should_discover_server(global_strategy, server_strategy)
}

/// Registry of MCP servers, pooled sessions, and discovered catalogs.
pub struct McpToolRegistry {
    config: McpConfig,
    load_strategy: McpLoadStrategy,
    tools: RwLock<Vec<McpToolDescriptor>>,
    resources: RwLock<Vec<McpResourceDescriptor>>,
    prompts: RwLock<Vec<McpPromptDescriptor>>,
    /// Servers that successfully listed resources (even if empty).
    resource_capable: RwLock<Vec<String>>,
    /// Servers that successfully listed prompts (even if empty).
    prompt_capable: RwLock<Vec<String>>,
    /// Servers that advertise SEP-2663 Tasks extension.
    task_capable: RwLock<Vec<String>>,
    pool: Arc<McpSessionPool>,
    report: RwLock<McpLoadReport>,
    policy: McpPolicyConfig,
    auth_store_path: Option<PathBuf>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<McpServerEvent>>>,
    /// True once tools (and resources/prompts) have been discovered for this registry.
    tools_discovered: RwLock<bool>,
    /// Options used for deferred discovery (lazy mode).
    load_options: RwLock<Option<McpLoadOptions>>,
    /// Servers that have already been individually discovered (even if full batch discovery is pending).
    discovered_servers: RwLock<Vec<String>>,
}

impl McpToolRegistry {
    pub fn empty() -> Self {
        Self {
            config: McpConfig::default(),
            load_strategy: McpLoadStrategy::Lazy,
            tools: RwLock::new(Vec::new()),
            resources: RwLock::new(Vec::new()),
            prompts: RwLock::new(Vec::new()),
            resource_capable: RwLock::new(Vec::new()),
            prompt_capable: RwLock::new(Vec::new()),
            task_capable: RwLock::new(Vec::new()),
            pool: Arc::new(McpSessionPool::new()),
            report: RwLock::new(McpLoadReport::default()),
            policy: McpPolicyConfig::default(),
            auth_store_path: None,
            event_rx: RwLock::new(None),
            tools_discovered: RwLock::new(false),
            load_options: RwLock::new(None),
            discovered_servers: RwLock::new(Vec::new()),
        }
    }

    /// Load with default options (continue on server errors, concurrency 4).
    pub async fn load(config: McpConfig) -> Result<Self> {
        Self::load_with_options(config, McpLoadOptions::default()).await
    }

    /// Discover tools (and optionally resources/prompts) from all enabled servers.
    ///
    /// When `options.load_strategy` is `lazy` (the default), servers are
    /// validated but not contacted; call [`McpToolRegistry::discover_tools`] or
    /// [`McpToolRegistry::create_agent_tools`] to trigger discovery later.
    pub async fn load_with_options(config: McpConfig, options: McpLoadOptions) -> Result<Self> {
        let pool = McpSessionPool::new()
            .with_auth_store_path(options.auth_store_path.clone())
            .with_response_cache(options.response_cache.clone())
            .with_cache_store(options.cache_store.clone())
            .with_default_cache_ttl(options.default_cache_ttl_ms);
        let (_event_tx, event_rx) = if options.enable_list_changed {
            let (tx, rx) = mpsc::unbounded_channel();
            pool.set_event_sender(tx.clone());
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let pool = Arc::new(pool);

        let enabled: Vec<(String, McpServerConfig)> = config
            .enabled_servers()
            .map(|(n, c)| (n.to_string(), c.clone()))
            .collect();
        let skipped = config.server_count().saturating_sub(enabled.len());
        let should_discover_any = enabled.iter().any(|(_, server_config)| {
            should_auto_discover_on_startup(options.load_strategy, server_config.load_strategy())
        });

        let (tools, resources, prompts, resource_capable, prompt_capable, task_capable, report) = if should_discover_any
        {
            let concurrency = options.max_concurrency.max(1);
            let pool_for_discovery = Arc::clone(&pool);
            let discover_rp = options.discover_resources_and_prompts;
            let progress_tx = options.progress_tx.clone();
            let total = enabled.len();
            let results: Vec<ServerDiscovery> = stream::iter(enabled.into_iter().enumerate())
                .map(|(index, (name, server_config))| {
                    let should_discover =
                        should_auto_discover_on_startup(options.load_strategy, server_config.load_strategy());
                    let discovery_timeout = options.discovery_timeout;
                    let pool = Arc::clone(&pool_for_discovery);
                    let progress_tx = progress_tx.clone();
                    async move {
                        if !should_discover {
                            return None;
                        }
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(McpServerLoadProgress::Started {
                                name: name.clone(),
                                index: index + 1,
                                total,
                            });
                        }
                        let result = discover_one(&pool, &name, server_config, discovery_timeout, discover_rp).await;
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(server_discovery_progress(&result));
                        }
                        Some(result)
                    }
                })
                .buffer_unordered(concurrency)
                .filter_map(|result| async move { result })
                .collect()
                .await;

            let (tools, resources, prompts, resource_capable, prompt_capable, task_capable, report) =
                build_catalogs_from_results(config.clone(), results, skipped, options.continue_on_error)?;
            (
                tools,
                resources,
                prompts,
                resource_capable,
                prompt_capable,
                task_capable,
                report,
            )
        } else {
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                McpLoadReport {
                    servers_skipped: skipped,
                    ..Default::default()
                },
            )
        };

        log::info!(
            "MCP registry loaded: tools={} resources={} prompts={} ok={} failed={} skipped={} strategy={}",
            report.tools_loaded,
            report.resources_loaded,
            report.prompts_loaded,
            report.servers_ok,
            report.servers_failed,
            report.servers_skipped,
            options.load_strategy.as_str()
        );

        Ok(Self {
            config: config.clone(),
            load_strategy: options.load_strategy,
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
            prompts: RwLock::new(prompts),
            resource_capable: RwLock::new(resource_capable),
            prompt_capable: RwLock::new(prompt_capable),
            task_capable: RwLock::new(task_capable),
            pool,
            report: RwLock::new(report),
            policy: config.policy.clone(),
            auth_store_path: options.auth_store_path.clone(),
            event_rx: RwLock::new(event_rx),
            tools_discovered: RwLock::new(should_discover_any),
            load_options: RwLock::new(Some(options)),
            discovered_servers: RwLock::new(Vec::new()),
        })
    }

    /// Ensure tools have been discovered from all enabled servers.
    ///
    /// For `lazy`-loaded registries, this triggers the deferred discovery.
    /// Subsequent calls are no-ops.
    pub fn is_server_discovered(&self, server_name: &str) -> bool {
        self.discovered_servers.read().iter().any(|n| n == server_name)
    }

    /// Count enabled servers that have not yet been discovered.
    pub fn pending_server_count(&self) -> usize {
        self.config
            .enabled_servers()
            .filter(|(n, _)| !self.is_server_discovered(n))
            .count()
    }

    /// Whether the full tool catalog has been discovered at least once.
    pub fn is_tools_discovered(&self) -> bool {
        *self.tools_discovered.read()
    }

    /// Load strategy used by this registry.
    pub fn load_strategy(&self) -> McpLoadStrategy {
        self.load_strategy
    }

    /// Stored load options (if any).
    pub fn load_options(&self) -> Option<McpLoadOptions> {
        self.load_options.read().clone()
    }

    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    pub fn policy(&self) -> &McpPolicyConfig {
        &self.policy
    }

    pub fn effective_policy_for(&self, server_name: &str) -> McpPolicyConfig {
        self.config
            .servers
            .get(server_name)
            .map(|s| self.config.effective_policy(s))
            .unwrap_or_else(|| self.policy.clone())
    }

    /// Whether a tool (by exposed name) requires approval under effective policy.
    pub fn tool_requires_approval(&self, exposed_name: &str) -> bool {
        if let Some(desc) = self.tools.read().iter().find(|t| t.exposed_name == exposed_name) {
            return desc.requires_approval;
        }
        mcp_tool_requires_approval(&self.policy, exposed_name)
    }

    pub fn descriptors(&self) -> Vec<McpToolDescriptor> {
        self.tools.read().clone()
    }

    pub fn resource_descriptors(&self) -> Vec<McpResourceDescriptor> {
        self.resources.read().clone()
    }

    pub fn prompt_descriptors(&self) -> Vec<McpPromptDescriptor> {
        self.prompts.read().clone()
    }

    pub fn tool_count(&self) -> usize {
        self.tools.read().len()
    }

    pub fn server_count(&self) -> usize {
        self.config.servers.len()
    }

    pub fn load_report(&self) -> McpLoadReport {
        self.report.read().clone()
    }

    pub fn session_pool(&self) -> &Arc<McpSessionPool> {
        &self.pool
    }

    pub fn auth_store_path(&self) -> Option<&PathBuf> {
        self.auth_store_path.as_ref()
    }

    /// Take the list_changed event receiver (at most once).
    pub fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<McpServerEvent>> {
        self.event_rx.write().take()
    }

    /// Ensure a specific server is discovered before tool call.
    ///
    /// Unlike `refresh_server()` (which re-discovers and replaces the session),
    /// this only triggers discovery if the server has not yet been discovered.
    /// This is called from the tool/resource/prompt call paths.
    ///
    /// Retries once on transient failures before giving up.
    async fn ensure_server_discovered(&self, server_name: &str) -> Result<()> {
        if self.is_server_discovered(server_name) {
            return Ok(());
        }
        let max_attempts = 2;
        for attempt in 1..=max_attempts {
            match self.discover_server(server_name).await {
                Ok(()) => return Ok(()),
                Err(error) if attempt < max_attempts => {
                    log::warn!(
                        "MCP server discovery failed; retrying: server={server_name} attempt={attempt}/{max_attempts} error={error}"
                    );
                    tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt as u32 - 1))).await;
                }
                Err(error) => {
                    return Err(error).context(format!(
                        "MCP server \"{server_name}\" unreachable after {max_attempts} attempts"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Call a tool on a configured server (pooled connection).
    pub async fn call_tool(&self, server: &str, tool_name: &str, args: Value) -> Result<AgentToolResult> {
        self.ensure_server_discovered(server).await?;
        let Some(server_config) = self.config.servers.get(server).cloned() else {
            anyhow::bail!("MCP server \"{server}\" not configured");
        };
        if server_config.is_disabled() {
            anyhow::bail!("MCP server \"{server}\" is disabled");
        }

        let result = self
            .pool
            .call_tool(server, server_config, tool_name, args)
            .await
            .with_context(|| format!("MCP tool {server}/{tool_name}"))?;
        Ok(mcp_result_to_agent(result))
    }

    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<AgentToolResult> {
        self.ensure_server_discovered(server).await?;
        let Some(server_config) = self.config.servers.get(server).cloned() else {
            anyhow::bail!("MCP server \"{server}\" not configured");
        };
        let contents = self
            .pool
            .read_resource(server, server_config, uri)
            .await
            .with_context(|| format!("MCP resource {server}/{uri}"))?;
        Ok(resource_contents_to_agent(contents))
    }

    pub async fn get_prompt(
        &self,
        server: &str,
        prompt_name: &str,
        arguments: Option<Value>,
    ) -> Result<AgentToolResult> {
        self.ensure_server_discovered(server).await?;
        let Some(server_config) = self.config.servers.get(server).cloned() else {
            anyhow::bail!("MCP server \"{server}\" not configured");
        };
        let result = self
            .pool
            .get_prompt(server, server_config, prompt_name, arguments)
            .await
            .with_context(|| format!("MCP prompt {server}/{prompt_name}"))?;
        let text = match serde_json::to_string_pretty(&result) {
            Ok(s) => s,
            Err(_) => format!("{result:?}"),
        };
        Ok(AgentToolResult::text(text))
    }

    /// One-shot call without using the pool (tests / doctor).
    pub async fn call_tool_ephemeral(&self, server: &str, tool_name: &str, args: Value) -> Result<AgentToolResult> {
        let Some(server_config) = self.config.servers.get(server) else {
            anyhow::bail!("MCP server \"{server}\" not configured");
        };
        let result = call_tool_for_server(server_config, tool_name, args).await?;
        Ok(mcp_result_to_agent(result))
    }

    /// Probe all enabled servers.
    pub async fn probe_all(&self) -> Vec<super::client::McpProbeResult> {
        let mut out = Vec::new();
        for (name, config) in self.config.enabled_servers() {
            out.push(probe_server_with_auth(name, config, self.auth_store_path.as_deref()).await);
        }
        out
    }

    /// Shut down pooled sessions.
    pub async fn shutdown(&self) {
        self.pool.close_all().await;
    }
}
/// Public helper for stable tool naming: `mcp_{server}__{tool}`.
pub fn expose_tool_name(server: &str, tool: &str) -> String {
    format!("mcp_{}__{}", sanitize_identifier(server), sanitize_identifier(tool))
}

/// Parse `mcp_{server}__{tool}` back into components when possible.
pub fn parse_exposed_tool_name(exposed: &str) -> Option<(&str, &str)> {
    let rest = exposed.strip_prefix("mcp_")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server, tool))
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

pub fn mcp_result_to_agent(result: CallToolResult) -> AgentToolResult {
    mcp_result_to_agent_with_limit(result, DEFAULT_MAX_TOOL_RESULT_CHARS)
}

/// Convert an MCP tool result, truncating each text block to `max_chars`.
pub fn mcp_result_to_agent_with_limit(result: CallToolResult, max_chars: usize) -> AgentToolResult {
    let is_error = result.is_error.unwrap_or(false);
    let mut content = Vec::new();

    for block in &result.content {
        match block {
            ContentBlock::Text(text) => {
                content.push(ToolResultContent::Text(elph_ai::TextContent::new(&text.text)));
            }
            ContentBlock::Image(image) => {
                content.push(ToolResultContent::Image(elph_ai::ImageContent::new(
                    &image.data,
                    &image.mime_type,
                )));
            }
            other => {
                if let Ok(text) = serde_json::to_string(other) {
                    content.push(ToolResultContent::Text(elph_ai::TextContent::new(text)));
                }
            }
        }
    }

    if content.is_empty() {
        if let Some(structured) = &result.structured_content {
            // Prefer structured.result string when present (DeepWiki style).
            let body = structured
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| structured.to_string());
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(body)));
        } else if is_error {
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(
                "MCP tool returned an error with no content",
            )));
        } else {
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(
                "MCP tool completed with no output",
            )));
        }
    }

    let truncated = truncate_tool_content(&mut content, max_chars);

    let text_joined = content
        .iter()
        .filter_map(|c| match c {
            ToolResultContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut agent_result = if is_error {
        AgentToolResult::error(if text_joined.is_empty() {
            "MCP tool returned an error".to_string()
        } else {
            text_joined
        })
    } else {
        AgentToolResult {
            content,
            details: Value::Null,
            added_tool_names: None,
            terminate: None,
            usage: None,
        }
    };

    // Keep details lean: flag only + truncated structured preview (no full duplicate body).
    let structured_preview = result
        .structured_content
        .as_ref()
        .map(|v| truncate_json_value(v, DEFAULT_MAX_STRUCTURED_DETAIL_CHARS));
    agent_result.details = json!({
        "mcp": true,
        "is_error": is_error,
        "truncated": truncated,
        "structured_content": structured_preview,
    });
    agent_result
}

fn resource_contents_to_agent(contents: Vec<ResourceContents>) -> AgentToolResult {
    let mut parts = Vec::new();
    for item in contents {
        match item {
            ResourceContents::TextResourceContents {
                uri, mime_type, text, ..
            } => {
                parts.push(format!("uri={uri} mime={mime_type:?}\n{text}"));
            }
            ResourceContents::BlobResourceContents {
                uri, mime_type, blob, ..
            } => {
                parts.push(format!("uri={uri} mime={mime_type:?} blob_bytes={}", blob.len()));
            }
            other => {
                parts.push(format!("{other:?}"));
            }
        }
    }
    let mut result = if parts.is_empty() {
        AgentToolResult::text("Resource returned no contents")
    } else {
        AgentToolResult::text(parts.join("\n---\n"))
    };
    let _ = truncate_tool_content(&mut result.content, DEFAULT_MAX_TOOL_RESULT_CHARS);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expose_tool_name_sanitizes() {
        assert_eq!(expose_tool_name("my-server", "read/file"), "mcp_my_server__read_file");
    }

    #[test]
    fn parse_exposed_roundtrip() {
        let name = expose_tool_name("fs", "read_file");
        assert_eq!(parse_exposed_tool_name(&name), Some(("fs", "read_file")));
        assert_eq!(parse_exposed_tool_name("not_mcp"), None);
    }

    #[test]
    fn eager_server_strategy_discover_is_honored() {
        assert!(should_discover_server(McpLoadStrategy::Lazy, McpLoadStrategy::Eager));
        assert!(should_discover_server(McpLoadStrategy::Eager, McpLoadStrategy::Lazy));
        assert!(!should_discover_server(McpLoadStrategy::Lazy, McpLoadStrategy::Lazy));
    }

    #[test]
    fn lazy_servers_skip_startup_discovery_until_requested() {
        assert!(!should_auto_discover_on_startup(McpLoadStrategy::Lazy, McpLoadStrategy::Lazy));
        assert!(should_auto_discover_on_startup(McpLoadStrategy::Lazy, McpLoadStrategy::Eager));
        assert!(should_auto_discover_on_startup(McpLoadStrategy::Eager, McpLoadStrategy::Lazy));
    }

    #[test]
    fn mcp_error_result_is_error_agent_tool() {
        let result = CallToolResult::error(vec![ContentBlock::text("boom")]);
        let agent = mcp_result_to_agent(result);
        let text = agent
            .content
            .iter()
            .filter_map(|c| match c {
                ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("boom"));
        assert_eq!(agent.details.get("is_error"), Some(&json!(true)));
    }

    #[test]
    fn truncates_large_tool_result() {
        let huge = "x".repeat(50_000);
        let result = CallToolResult::success(vec![ContentBlock::text(huge)]);
        let agent = mcp_result_to_agent_with_limit(result, 100);
        let text = agent
            .content
            .iter()
            .filter_map(|c| match c {
                ToolResultContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("truncated"));
        assert!(text.chars().count() < 50_000);
        assert_eq!(agent.details.get("truncated"), Some(&json!(true)));
    }

    #[test]
    fn empty_registry_is_lazy_and_undiscovered() {
        let registry = McpToolRegistry::empty();
        assert_eq!(registry.load_strategy(), McpLoadStrategy::Lazy);
        assert!(!registry.is_tools_discovered());
        assert_eq!(registry.pending_server_count(), 0);
        assert!(registry.load_options().is_none());
    }

    #[test]
    fn pending_count_reflects_individual_discovery() {
        let mut registry = McpToolRegistry::empty();
        assert_eq!(registry.pending_server_count(), 0);

        registry.discovered_servers.write().push("fs".to_string());
        assert_eq!(registry.pending_server_count(), 0);

        // Simulate a configured but not-yet-discovered server.
        registry
            .config
            .servers
            .insert("web".into(), McpServerConfig::stdio("echo", vec![]));
        assert_eq!(registry.pending_server_count(), 1);

        registry.discovered_servers.write().push("web".to_string());
        assert_eq!(registry.pending_server_count(), 0);
    }

    #[test]
    fn is_server_discovered_tracks_individual_servers() {
        let registry = McpToolRegistry::empty();
        assert!(!registry.is_server_discovered("fs"));
        registry.discovered_servers.write().push("fs".to_string());
        assert!(registry.is_server_discovered("fs"));
        assert!(!registry.is_server_discovered("other"));
    }
}
