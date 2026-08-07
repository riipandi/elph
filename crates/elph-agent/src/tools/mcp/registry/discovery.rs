use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use rmcp::model::{Prompt, Resource, Tool as McpTool};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::super::client::validate_server_config;
use super::super::config::{McpConfig, McpServerConfig, McpServerLoadProgress};
use super::super::events::McpServerEvent;
use super::super::policy::mcp_tool_requires_approval;
use super::super::session::McpSessionPool;
use super::McpToolRegistry;
use super::expose_tool_name;
use super::{McpLoadReport, McpPromptDescriptor, McpResourceDescriptor, McpServerLoadReport, McpToolDescriptor};

impl McpToolRegistry {
    pub async fn discover_tools(&self) -> Result<()> {
        self.discover_tools_with_options(None, None, None, None).await
    }

    /// Discover tools with an optional progress reporter override.
    ///
    /// This is useful for deferred/bootstrap discovery where the caller wants
    /// progress events without having preconfigured `McpLoadOptions`.
    pub async fn discover_tools_with_progress(
        &self,
        progress_tx: Option<mpsc::UnboundedSender<McpServerLoadProgress>>,
    ) -> Result<()> {
        self.discover_tools_with_options(progress_tx, None, None, None).await
    }

    async fn discover_tools_with_options(
        &self,
        progress_override: Option<mpsc::UnboundedSender<McpServerLoadProgress>>,
        concurrency_override: Option<usize>,
        continue_on_error_override: Option<bool>,
        timeout_override: Option<Duration>,
    ) -> Result<()> {
        // Re-run whenever any enabled server still needs discovery.
        // (A prior partial pass must not block later ensure/on-demand loads.)
        let enabled: Vec<(String, McpServerConfig)> = self
            .config
            .enabled_servers()
            .map(|(n, c)| (n.to_string(), c.clone()))
            .collect();
        let skipped = self.config.server_count().saturating_sub(enabled.len());
        let pending: Vec<(String, McpServerConfig)> = enabled
            .into_iter()
            .filter(|(n, _)| !self.is_server_discovered(n))
            .collect();

        if pending.is_empty() {
            *self.tools_discovered.write() = true;
            return Ok(());
        }

        // Serialize concurrent discovery of the same pending set.
        {
            let mut discovered = self.tools_discovered.write();
            // Soft flag: true while a full pass is "complete enough"; reset if we still
            // have pending work so concurrent callers don't bail early.
            if *discovered && pending.is_empty() {
                return Ok(());
            }
            *discovered = false;
        }

        let result: Result<()> = async {
            let pending = pending;

            let (concurrency, continue_on_error, discover_rp, progress_tx, discovery_timeout) = {
                let options = self.load_options.read();
                let opts = options.as_ref();
                (
                    concurrency_override.unwrap_or_else(|| opts.map_or(4, |o| o.max_concurrency.max(1))),
                    continue_on_error_override.unwrap_or_else(|| opts.is_none_or(|o| o.continue_on_error)),
                    opts.is_none_or(|o| o.discover_resources_and_prompts),
                    progress_override.or_else(|| opts.and_then(|o| o.progress_tx.clone())),
                    timeout_override.or_else(|| opts.and_then(|o| o.discovery_timeout)),
                )
            };

            let total = pending.len();
            let results: Vec<ServerDiscovery> = stream::iter(pending.into_iter().enumerate())
                .map(|(index, (name, server_config))| {
                    let pool = Arc::clone(&self.pool);
                    let progress_tx = progress_tx.clone();
                    async move {
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
                        if matches!(result, ServerDiscovery::Ok { .. }) {
                            let mut discovered = self.discovered_servers.write();
                            if !discovered.contains(&name) {
                                discovered.push(name.clone());
                            }
                        }
                        result
                    }
                })
                .buffer_unordered(concurrency)
                .collect()
                .await;
            let (
                new_tools,
                new_resources,
                new_prompts,
                new_resource_capable,
                new_prompt_capable,
                new_task_capable,
                report,
            ) = build_catalogs_from_results(self.config.clone(), results, skipped, continue_on_error)?;

            // Merge newly discovered items into existing catalogs (preserve prior discoveries).
            {
                let mut tools = self.tools.write();
                for tool in &new_tools {
                    tools.retain(|t| t.server_name != tool.server_name);
                }
                tools.extend(new_tools);
            }
            // Resources: retain entries for servers not in the newly discovered set.
            {
                let mut resources = self.resources.write();
                resources.retain(|r| !new_resource_capable.contains(&r.server_name));
                resources.extend(new_resources);
            }
            {
                let mut prompts = self.prompts.write();
                prompts.retain(|p| !new_prompt_capable.contains(&p.server_name));
                prompts.extend(new_prompts);
            }
            // Capabilities: add new, avoid duplicates.
            for server in &new_resource_capable {
                let mut caps = self.resource_capable.write();
                if !caps.contains(server) {
                    caps.push(server.clone());
                }
            }
            for server in &new_prompt_capable {
                let mut caps = self.prompt_capable.write();
                if !caps.contains(server) {
                    caps.push(server.clone());
                }
            }
            for server in &new_task_capable {
                let mut caps = self.task_capable.write();
                if !caps.contains(server) {
                    caps.push(server.clone());
                }
            }
            // Update total report counts.
            *self.report.write() = report;
            Ok(())
        }
        .await;

        // Only mark "fully discovered" when every enabled server succeeded at least once.
        let all_done = self.config.enabled_servers().all(|(n, _)| self.is_server_discovered(n));
        *self.tools_discovered.write() = result.is_ok() && all_done;
        result
    }

    /// Discover a single server on-demand (lazy mode friendly).
    ///
    /// This is useful when you want to expose tools for one specific server
    /// without waiting for the full batch discovery. Results are merged into
    /// the existing catalog (other servers' tools are preserved).
    pub async fn discover_server(&self, server_name: &str) -> Result<()> {
        {
            let discovered = self.discovered_servers.read();
            if discovered.contains(&server_name.to_string()) {
                return Ok(());
            }
        }

        let Some(server_config) = self.config.servers.get(server_name).cloned() else {
            anyhow::bail!("MCP server \"{server_name}\" not configured");
        };
        if server_config.is_disabled() {
            anyhow::bail!("MCP server \"{server_name}\" is disabled");
        }

        let (discover_rp, discovery_timeout) = {
            let options = self.load_options.read();
            let opts = options.as_ref();
            (
                opts.is_none_or(|o| o.discover_resources_and_prompts),
                opts.and_then(|o| o.discovery_timeout),
            )
        };

        let result = discover_one(&self.pool, server_name, server_config, discovery_timeout, discover_rp).await;

        // Merge discovered items into existing catalogs (preserve other servers).
        let (
            new_tools,
            new_resources,
            new_prompts,
            new_resource_capable,
            new_prompt_capable,
            new_task_capable,
            _report,
        ) = build_catalogs_from_results(self.config.clone(), vec![result], 0, true)?;

        {
            let mut discovered = self.discovered_servers.write();
            if !discovered.iter().any(|n| n == server_name) {
                discovered.push(server_name.to_string());
            }
        }
        {
            let mut tools = self.tools.write();
            tools.retain(|t| t.server_name != server_name);
            tools.extend(new_tools);
        }
        {
            let mut resources = self.resources.write();
            resources.retain(|r| r.server_name != server_name);
            resources.extend(new_resources);
        }
        {
            let mut prompts = self.prompts.write();
            prompts.retain(|p| p.server_name != server_name);
            prompts.extend(new_prompts);
        }
        {
            let mut caps = self.resource_capable.write();
            let name = server_name.to_string();
            if new_resource_capable.contains(&name) && !caps.contains(&name) {
                caps.push(name);
            }
        }
        {
            let mut caps = self.prompt_capable.write();
            let name = server_name.to_string();
            if new_prompt_capable.contains(&name) && !caps.contains(&name) {
                caps.push(name);
            }
        }
        {
            let mut caps = self.task_capable.write();
            let name = server_name.to_string();
            if new_task_capable.contains(&name) && !caps.contains(&name) {
                caps.push(name);
            }
        }
        // Update total report counts.
        {
            let mut report = self.report.write();
            report.servers_ok = report.servers_ok.saturating_add(1);
            report.tools_loaded = self.tools.read().len();
            report.resources_loaded = self.resources.read().len();
            report.prompts_loaded = self.prompts.read().len();
        }
        Ok(())
    }
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
            McpServerEvent::Progress { .. } | McpServerEvent::TaskStatus { .. } => return Ok(0),
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
                    McpServerEvent::TaskStatus {
                        server,
                        task_id,
                        status,
                        status_message,
                    } => {
                        log::info!(
                            "MCP task status: server={server} task_id={task_id} status={status} message={status_message:?}"
                        );
                    }
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
}

pub(crate) fn server_discovery_progress(result: &ServerDiscovery) -> McpServerLoadProgress {
    match result {
        ServerDiscovery::Ok {
            name,
            transport,
            descriptors,
            message,
            ..
        } => McpServerLoadProgress::Finished {
            name: name.clone(),
            ok: true,
            transport: transport.clone(),
            tool_count: descriptors.len(),
            message: message.clone(),
        },
        ServerDiscovery::Failed { name, transport, error } => McpServerLoadProgress::Finished {
            name: name.clone(),
            ok: false,
            transport: transport.clone(),
            tool_count: 0,
            message: error.clone(),
        },
    }
}

pub(crate) enum ServerDiscovery {
    Ok {
        name: String,
        transport: String,
        descriptors: Vec<McpToolDescriptor>,
        resource_descriptors: Vec<McpResourceDescriptor>,
        prompt_descriptors: Vec<McpPromptDescriptor>,
        resources_ok: bool,
        prompts_ok: bool,
        tasks_ok: bool,
        message: String,
    },
    Failed {
        name: String,
        transport: String,
        error: String,
    },
}

/// Catalogs produced by [`build_catalogs_from_results`]:
/// (tools, resources, prompts, resource_capable, prompt_capable, task_capable, report).
pub(crate) type DiscoveredCatalogs = (
    Vec<McpToolDescriptor>,
    Vec<McpResourceDescriptor>,
    Vec<McpPromptDescriptor>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    McpLoadReport,
);

pub(crate) fn build_catalogs_from_results(
    config: McpConfig,
    results: Vec<ServerDiscovery>,
    skipped: usize,
    continue_on_error: bool,
) -> Result<DiscoveredCatalogs> {
    let mut tools = Vec::new();
    let mut resources = Vec::new();
    let mut prompts = Vec::new();
    let mut resource_capable = Vec::new();
    let mut prompt_capable = Vec::new();
    let mut task_capable = Vec::new();
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
                tasks_ok,
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
                    prompt_capable.push(name.clone());
                }
                if tasks_ok {
                    task_capable.push(name);
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
                if continue_on_error {
                    log::warn!("MCP server discovery failed; continuing: server={name} error={error}");
                } else {
                    anyhow::bail!("MCP server \"{name}\" discovery failed: {error}");
                }
            }
        }
    }

    // Apply global policy for requires_approval flags and drop denied tools.
    let policy = config.policy.clone();
    for tool in &mut tools {
        let server_cfg = config.servers.get(&tool.server_name);
        let effective = server_cfg
            .map(|s| config.effective_policy(s))
            .unwrap_or_else(|| policy.clone());
        tool.requires_approval = mcp_tool_requires_approval(&effective, &tool.exposed_name);
    }
    tools.retain(|t| {
        let server_cfg = config.servers.get(&t.server_name);
        let effective = server_cfg
            .map(|s| config.effective_policy(s))
            .unwrap_or_else(|| policy.clone());
        effective.is_exposed(&t.exposed_name)
    });

    Ok((
        tools,
        resources,
        prompts,
        resource_capable,
        prompt_capable,
        task_capable,
        report,
    ))
}

pub(crate) async fn discover_one(
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

        let tasks_ok = match pool
            .get_or_insert(server_name, config.clone())
            .await
            .supports_tasks()
            .await
        {
            Ok(v) => v,
            Err(error) => {
                debug_ignore_capability(server_name, "tasks", &error);
                false
            }
        };

        Ok::<_, anyhow::Error>((
            tools,
            resource_descriptors,
            prompt_descriptors,
            resources_ok,
            prompts_ok,
            tasks_ok,
        ))
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
        Ok((remote_tools, resource_descriptors, prompt_descriptors, resources_ok, prompts_ok, tasks_ok)) => {
            let descriptors: Vec<_> = remote_tools
                .into_iter()
                .map(|tool| descriptor_from_mcp(server_name, &tool))
                .collect();
            let count = descriptors.len();
            let mut message = format!("discovered {count} tools");
            if tasks_ok {
                message.push_str(", tasks extension");
            }
            ServerDiscovery::Ok {
                name: server_name.to_string(),
                transport,
                descriptors,
                resource_descriptors,
                prompt_descriptors,
                resources_ok,
                prompts_ok,
                tasks_ok,
                message,
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
