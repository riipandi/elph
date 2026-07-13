//! MCP tool registry — discover remote tools/resources/prompts and bridge them into the agent harness.
//!
//! Production path:
//! 1. [`McpToolRegistry::load`] / [`load_with_options`] discovers catalogs (fail-open by default).
//! 2. Sessions are pooled so tool calls reuse stdio processes / HTTP sessions.
//! 3. [`McpToolRegistry::create_agent_tools`] exposes `mcp_{server}__{tool}` agent tools.
//! 4. Policy filters deny-listed tools; approval is enforced via [`crate::mcp::policy`].
//! 5. `tools/list_changed` (and resource/prompt variants) can refresh catalogs in place.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use elph_ai::Tool;
use futures::stream::{self, StreamExt};
use parking_lot::RwLock;
use rmcp::model::{CallToolResult, ContentBlock, Prompt, Resource, ResourceContents, Tool as McpTool};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult, ToolResultContent};

use super::client::{call_tool_for_server, probe_server_with_auth, validate_server_config};
use super::config::{McpConfig, McpLoadOptions, McpServerConfig};
use super::events::McpServerEvent;
use super::policy::{McpPolicyConfig, mcp_tool_requires_approval};
use super::session::McpSessionPool;
use super::truncate::{
    DEFAULT_MAX_STRUCTURED_DETAIL_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS, truncate_json_value, truncate_tool_content,
};

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

/// Registry of MCP servers, pooled sessions, and discovered catalogs.
pub struct McpToolRegistry {
    config: McpConfig,
    tools: RwLock<Vec<McpToolDescriptor>>,
    resources: RwLock<Vec<McpResourceDescriptor>>,
    prompts: RwLock<Vec<McpPromptDescriptor>>,
    /// Servers that successfully listed resources (even if empty).
    resource_capable: RwLock<Vec<String>>,
    /// Servers that successfully listed prompts (even if empty).
    prompt_capable: RwLock<Vec<String>>,
    pool: Arc<McpSessionPool>,
    report: RwLock<McpLoadReport>,
    policy: McpPolicyConfig,
    auth_store_path: Option<PathBuf>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<McpServerEvent>>>,
}

impl McpToolRegistry {
    pub fn empty() -> Self {
        Self {
            config: McpConfig::default(),
            tools: RwLock::new(Vec::new()),
            resources: RwLock::new(Vec::new()),
            prompts: RwLock::new(Vec::new()),
            resource_capable: RwLock::new(Vec::new()),
            prompt_capable: RwLock::new(Vec::new()),
            pool: Arc::new(McpSessionPool::new()),
            report: RwLock::new(McpLoadReport::default()),
            policy: McpPolicyConfig::default(),
            auth_store_path: None,
            event_rx: RwLock::new(None),
        }
    }

    /// Load with default options (continue on server errors, concurrency 4).
    pub async fn load(config: McpConfig) -> Result<Self> {
        Self::load_with_options(config, McpLoadOptions::default()).await
    }

    /// Discover tools (and optionally resources/prompts) from all enabled servers.
    pub async fn load_with_options(config: McpConfig, options: McpLoadOptions) -> Result<Self> {
        let pool = McpSessionPool::new().with_auth_store_path(options.auth_store_path.clone());
        let (event_tx, event_rx) = if options.enable_list_changed {
            let (tx, rx) = mpsc::unbounded_channel();
            pool.set_event_sender(tx.clone());
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let _ = event_tx;
        let pool = Arc::new(pool);

        let enabled: Vec<(String, McpServerConfig)> = config
            .enabled_servers()
            .map(|(n, c)| (n.to_string(), c.clone()))
            .collect();
        let skipped = config.server_count().saturating_sub(enabled.len());

        let concurrency = options.max_concurrency.max(1);
        let pool_for_discovery = Arc::clone(&pool);
        let discover_rp = options.discover_resources_and_prompts;
        let results: Vec<ServerDiscovery> = stream::iter(enabled)
            .map(|(name, server_config)| {
                let discovery_timeout = options.discovery_timeout;
                let pool = Arc::clone(&pool_for_discovery);
                async move { discover_one(&pool, &name, server_config, discovery_timeout, discover_rp).await }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        let mut tools = Vec::new();
        let mut resources = Vec::new();
        let mut prompts = Vec::new();
        let mut resource_capable = Vec::new();
        let mut prompt_capable = Vec::new();
        let mut report = McpLoadReport {
            servers_skipped: skipped,
            ..Default::default()
        };

        for result in results {
            match result {
                ServerDiscovery::Ok {
                    name,
                    transport,
                    descriptors,
                    resource_descriptors,
                    prompt_descriptors,
                    resources_ok,
                    prompts_ok,
                    message,
                } => {
                    report.servers_ok += 1;
                    report.tools_loaded += descriptors.len();
                    report.resources_loaded += resource_descriptors.len();
                    report.prompts_loaded += prompt_descriptors.len();
                    report.servers.push(McpServerLoadReport {
                        name: name.clone(),
                        ok: true,
                        transport,
                        tool_count: descriptors.len(),
                        resource_count: resource_descriptors.len(),
                        prompt_count: prompt_descriptors.len(),
                        message,
                    });
                    tools.extend(descriptors);
                    resources.extend(resource_descriptors);
                    prompts.extend(prompt_descriptors);
                    if resources_ok {
                        resource_capable.push(name.clone());
                    }
                    if prompts_ok {
                        prompt_capable.push(name);
                    }
                }
                ServerDiscovery::Failed { name, transport, error } => {
                    report.servers_failed += 1;
                    report.servers.push(McpServerLoadReport {
                        name: name.clone(),
                        ok: false,
                        transport,
                        tool_count: 0,
                        resource_count: 0,
                        prompt_count: 0,
                        message: error.clone(),
                    });
                    if options.continue_on_error {
                        log::warn!("MCP server discovery failed; continuing: server={name} error={error}");
                    } else {
                        anyhow::bail!("MCP server \"{name}\" discovery failed: {error}");
                    }
                }
            }
        }

        // Apply global policy for requires_approval flags
        let policy = config.policy.clone();
        for tool in &mut tools {
            let server_cfg = config.servers.get(&tool.server_name);
            let effective = server_cfg
                .map(|s| config.effective_policy(s))
                .unwrap_or_else(|| policy.clone());
            tool.requires_approval = mcp_tool_requires_approval(&effective, &tool.exposed_name);
            // Drop denied tools
        }
        tools.retain(|t| {
            let server_cfg = config.servers.get(&t.server_name);
            let effective = server_cfg
                .map(|s| config.effective_policy(s))
                .unwrap_or_else(|| policy.clone());
            effective.is_exposed(&t.exposed_name)
        });

        log::info!(
            "MCP registry loaded: tools={} resources={} prompts={} ok={} failed={} skipped={}",
            report.tools_loaded,
            report.resources_loaded,
            report.prompts_loaded,
            report.servers_ok,
            report.servers_failed,
            report.servers_skipped
        );

        Ok(Self {
            config,
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
            prompts: RwLock::new(prompts),
            resource_capable: RwLock::new(resource_capable),
            prompt_capable: RwLock::new(prompt_capable),
            pool,
            report: RwLock::new(report),
            policy,
            auth_store_path: options.auth_store_path,
            event_rx: RwLock::new(event_rx),
        })
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

    /// Refresh tools (and resources/prompts) for a single server.
    pub async fn refresh_server(&self, server_name: &str) -> Result<usize> {
        let Some(server_config) = self.config.servers.get(server_name).cloned() else {
            anyhow::bail!("MCP server \"{server_name}\" not configured");
        };
        if server_config.is_disabled() {
            self.tools.write().retain(|t| t.server_name != server_name);
            self.resources.write().retain(|t| t.server_name != server_name);
            self.prompts.write().retain(|t| t.server_name != server_name);
            self.resource_capable.write().retain(|n| n != server_name);
            self.prompt_capable.write().retain(|n| n != server_name);
            let _ = self.pool.remove(server_name).await;
            return Ok(0);
        }

        let _ = self.pool.remove(server_name).await;
        let discovery = discover_one(&self.pool, server_name, server_config, None, true).await;
        match discovery {
            ServerDiscovery::Ok {
                descriptors,
                resource_descriptors,
                prompt_descriptors,
                resources_ok,
                prompts_ok,
                ..
            } => {
                let policy = self.effective_policy_for(server_name);
                let mut tools: Vec<_> = descriptors
                    .into_iter()
                    .map(|mut d| {
                        d.requires_approval = mcp_tool_requires_approval(&policy, &d.exposed_name);
                        d
                    })
                    .filter(|d| policy.is_exposed(&d.exposed_name))
                    .collect();
                let count = tools.len();

                {
                    let mut all = self.tools.write();
                    all.retain(|t| t.server_name != server_name);
                    all.append(&mut tools);
                }
                {
                    let mut all = self.resources.write();
                    all.retain(|t| t.server_name != server_name);
                    all.extend(resource_descriptors);
                }
                {
                    let mut all = self.prompts.write();
                    all.retain(|t| t.server_name != server_name);
                    all.extend(prompt_descriptors);
                }
                {
                    let mut caps = self.resource_capable.write();
                    caps.retain(|n| n != server_name);
                    if resources_ok {
                        caps.push(server_name.to_string());
                    }
                }
                {
                    let mut caps = self.prompt_capable.write();
                    caps.retain(|n| n != server_name);
                    if prompts_ok {
                        caps.push(server_name.to_string());
                    }
                }
                Ok(count)
            }
            ServerDiscovery::Failed { error, .. } => Err(anyhow::anyhow!(error)),
        }
    }

    /// Apply a list_changed (or related) event by refreshing the affected server.
    pub async fn handle_event(&self, event: &McpServerEvent) -> Result<usize> {
        let server = match event {
            McpServerEvent::ToolListChanged { server }
            | McpServerEvent::ResourceListChanged { server }
            | McpServerEvent::PromptListChanged { server }
            | McpServerEvent::ResourceUpdated { server, .. } => server.as_str(),
            McpServerEvent::Progress { .. } => return Ok(0),
        };
        log::info!("MCP catalog change; refreshing server: server={server} event={event:?}");
        self.refresh_server(server).await
    }

    /// Spawn a background task for catalog changes and progress notifications.
    ///
    /// - Catalog events (`tools/list_changed`, etc.) refresh the server then call `on_refresh`.
    /// - Progress events call `on_progress` with a short status line for the UI.
    pub fn spawn_event_loop<F, P>(self: &Arc<Self>, mut on_refresh: F, mut on_progress: P) -> bool
    where
        F: FnMut(Arc<McpToolRegistry>) + Send + 'static,
        P: FnMut(String) + Send + 'static,
    {
        let Some(mut rx) = self.take_event_receiver() else {
            return false;
        };
        let registry = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match &event {
                    McpServerEvent::Progress {
                        server,
                        progress,
                        total,
                        message,
                    } => {
                        let line = format_progress_status(server, *progress, *total, message.as_deref());
                        log::info!("MCP progress: server={server} progress={progress} total={total:?}");
                        on_progress(line);
                    }
                    McpServerEvent::ToolListChanged { .. }
                    | McpServerEvent::ResourceListChanged { .. }
                    | McpServerEvent::PromptListChanged { .. }
                    | McpServerEvent::ResourceUpdated { .. } => match registry.handle_event(&event).await {
                        Ok(_) => on_refresh(Arc::clone(&registry)),
                        Err(error) => {
                            log::warn!("MCP hot reload failed: {error}");
                        }
                    },
                }
            }
        });
        true
    }

    /// Spawn catalog hot-reload only (progress goes to logging).
    pub fn spawn_hot_reload<F>(self: &Arc<Self>, on_refresh: F) -> bool
    where
        F: FnMut(Arc<McpToolRegistry>) + Send + 'static,
    {
        self.spawn_event_loop(on_refresh, |_msg| {})
    }

    /// Convert discovered MCP tools (+ resource/prompt bridge tools) into harness [`AgentTool`]s.
    pub fn create_agent_tools(self: &Arc<Self>) -> Vec<AgentTool> {
        let mut out = Vec::new();

        for desc in self.tools.read().iter() {
            let registry = Arc::clone(self);
            let server = desc.server_name.clone();
            let tool_name = desc.tool_name.clone();
            out.push(simple_tool(
                Tool {
                    name: desc.exposed_name.clone(),
                    description: desc.description.clone(),
                    parameters: desc.parameters.clone(),
                },
                format!("MCP:{}", desc.server_name),
                move |_, args| {
                    let registry = registry.clone();
                    let server = server.clone();
                    let tool_name = tool_name.clone();
                    Box::pin(async move { registry.call_tool(&server, &tool_name, args).await })
                },
            ));
        }

        // Bridge tools for resources / prompts per capable server.
        for server in self.resource_capable.read().iter() {
            out.push(self.bridge_list_resources(server));
            out.push(self.bridge_read_resource(server));
        }
        for server in self.prompt_capable.read().iter() {
            out.push(self.bridge_list_prompts(server));
            out.push(self.bridge_get_prompt(server));
        }

        out
    }

    fn bridge_list_resources(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "list_resources");
        simple_tool(
            Tool {
                name,
                description: format!("[MCP:{server}] List resources available on this MCP server"),
                parameters: json!({ "type": "object", "properties": {} }),
            },
            format!("MCP:{server}"),
            move |_, _| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let items = registry.resources.read().clone();
                    let filtered: Vec<_> = items.into_iter().filter(|r| r.server_name == server).collect();
                    let payload = json!(
                        filtered
                            .iter()
                            .map(|r| json!({
                                "uri": r.uri,
                                "name": r.name,
                                "description": r.description,
                                "mimeType": r.mime_type,
                            }))
                            .collect::<Vec<_>>()
                    );
                    Ok(AgentToolResult::text(payload.to_string()))
                })
            },
        )
    }

    fn bridge_read_resource(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "read_resource");
        simple_tool(
            Tool {
                name,
                description: format!("[MCP:{server}] Read a resource by URI from this MCP server"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uri": { "type": "string", "description": "Resource URI" }
                    },
                    "required": ["uri"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let uri = args
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .context("uri is required")?
                        .to_string();
                    registry.read_resource(&server, &uri).await
                })
            },
        )
    }

    fn bridge_list_prompts(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "list_prompts");
        simple_tool(
            Tool {
                name,
                description: format!("[MCP:{server}] List prompt templates on this MCP server"),
                parameters: json!({ "type": "object", "properties": {} }),
            },
            format!("MCP:{server}"),
            move |_, _| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let items = registry.prompts.read().clone();
                    let filtered: Vec<_> = items.into_iter().filter(|p| p.server_name == server).collect();
                    let payload = json!(
                        filtered
                            .iter()
                            .map(|p| json!({
                                "name": p.name,
                                "description": p.description,
                                "arguments": p.arguments_schema,
                            }))
                            .collect::<Vec<_>>()
                    );
                    Ok(AgentToolResult::text(payload.to_string()))
                })
            },
        )
    }

    fn bridge_get_prompt(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "get_prompt");
        simple_tool(
            Tool {
                name,
                description: format!("[MCP:{server}] Fetch a prompt template with optional arguments"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Prompt name" },
                        "arguments": { "type": "object", "description": "Prompt arguments" }
                    },
                    "required": ["name"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let prompt_name = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .context("name is required")?
                        .to_string();
                    let arguments = args.get("arguments").cloned();
                    registry.get_prompt(&server, &prompt_name, arguments).await
                })
            },
        )
    }

    /// Call a tool on a configured server (pooled connection).
    pub async fn call_tool(&self, server: &str, tool_name: &str, args: Value) -> Result<AgentToolResult> {
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

enum ServerDiscovery {
    Ok {
        name: String,
        transport: String,
        descriptors: Vec<McpToolDescriptor>,
        resource_descriptors: Vec<McpResourceDescriptor>,
        prompt_descriptors: Vec<McpPromptDescriptor>,
        resources_ok: bool,
        prompts_ok: bool,
        message: String,
    },
    Failed {
        name: String,
        transport: String,
        error: String,
    },
}

async fn discover_one(
    pool: &McpSessionPool,
    server_name: &str,
    config: McpServerConfig,
    override_timeout: Option<std::time::Duration>,
    discover_rp: bool,
) -> ServerDiscovery {
    let transport = config.kind_label().to_string();
    if let Err(error) = validate_server_config(server_name, &config) {
        return ServerDiscovery::Failed {
            name: server_name.to_string(),
            transport,
            error: error.to_string(),
        };
    }

    let op = async {
        let tools = pool
            .list_tools(server_name, config.clone())
            .await
            .with_context(|| format!("list tools from MCP server \"{server_name}\""))?;

        let mut resources_ok = false;
        let mut resource_descriptors = Vec::new();
        let mut prompts_ok = false;
        let mut prompt_descriptors = Vec::new();

        if discover_rp {
            match pool.list_resources(server_name, config.clone()).await {
                Ok(resources) => {
                    resources_ok = true;
                    resource_descriptors = resources
                        .into_iter()
                        .map(|r| resource_descriptor(server_name, &r))
                        .collect();
                }
                Err(error) => {
                    debug_ignore_capability(server_name, "resources", &error);
                }
            }
            match pool.list_prompts(server_name, config.clone()).await {
                Ok(prompts) => {
                    prompts_ok = true;
                    prompt_descriptors = prompts
                        .into_iter()
                        .map(|p| prompt_descriptor(server_name, &p))
                        .collect();
                }
                Err(error) => {
                    debug_ignore_capability(server_name, "prompts", &error);
                }
            }
        }

        Ok::<_, anyhow::Error>((tools, resource_descriptors, prompt_descriptors, resources_ok, prompts_ok))
    };

    let result = if let Some(t) = override_timeout {
        match tokio::time::timeout(t, op).await {
            Ok(inner) => inner,
            Err(_) => Err(anyhow::anyhow!("discovery timed out after {t:?}")),
        }
    } else {
        op.await
    };

    match result {
        Ok((remote_tools, resource_descriptors, prompt_descriptors, resources_ok, prompts_ok)) => {
            let descriptors: Vec<_> = remote_tools
                .into_iter()
                .map(|tool| descriptor_from_mcp(server_name, &tool))
                .collect();
            let count = descriptors.len();
            ServerDiscovery::Ok {
                name: server_name.to_string(),
                transport,
                descriptors,
                resource_descriptors,
                prompt_descriptors,
                resources_ok,
                prompts_ok,
                message: format!("discovered {count} tools"),
            }
        }
        Err(error) => ServerDiscovery::Failed {
            name: server_name.to_string(),
            transport,
            error: error.to_string(),
        },
    }
}

fn debug_ignore_capability(server: &str, kind: &str, error: &anyhow::Error) {
    log::debug!("MCP capability not available: server={server} kind={kind} error={error}");
}

fn format_progress_status(server: &str, progress: f64, total: Option<f64>, message: Option<&str>) -> String {
    let pct = match total {
        Some(t) if t > 0.0 => format!(" ({:.0}%)", (progress / t * 100.0).clamp(0.0, 100.0)),
        _ => String::new(),
    };
    match message {
        Some(m) if !m.is_empty() => format!("MCP:{server}{pct} — {m}"),
        _ => format!("MCP:{server}{pct} progress={progress}"),
    }
}

fn descriptor_from_mcp(server_name: &str, tool: &McpTool) -> McpToolDescriptor {
    let tool_name = tool.name.to_string();
    let exposed_name = expose_tool_name(server_name, &tool_name);
    let description = tool
        .description
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("MCP tool from server {server_name}"));
    let full_description = format!("[MCP:{server_name}] {description}");
    let parameters = {
        let schema = (*tool.input_schema).clone();
        if schema.is_empty() {
            json!({ "type": "object", "properties": {} })
        } else {
            Value::Object(schema)
        }
    };
    McpToolDescriptor {
        server_name: server_name.to_string(),
        tool_name,
        exposed_name,
        description: full_description,
        parameters,
        requires_approval: true,
    }
}

fn resource_descriptor(server_name: &str, resource: &Resource) -> McpResourceDescriptor {
    McpResourceDescriptor {
        server_name: server_name.to_string(),
        uri: resource.uri.clone(),
        name: resource.name.clone(),
        description: resource.description.clone().unwrap_or_default(),
        mime_type: resource.mime_type.clone(),
    }
}

fn prompt_descriptor(server_name: &str, prompt: &Prompt) -> McpPromptDescriptor {
    let arguments_schema = prompt
        .arguments
        .as_ref()
        .map(|args| serde_json::to_value(args).unwrap_or(json!([])))
        .unwrap_or(json!([]));
    McpPromptDescriptor {
        server_name: server_name.to_string(),
        name: prompt.name.clone(),
        description: prompt.description.clone().unwrap_or_default(),
        arguments_schema,
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
}
