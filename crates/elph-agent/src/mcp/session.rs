//! Long-lived MCP client sessions with reconnect-on-failure.
//!
//! Production tool calls reuse these sessions instead of spawning a new
//! stdio process (or HTTP handshake) for every invocation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rmcp::model::{CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, Tool};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio::time::timeout;

use super::client::{
    McpClient, McpConnectContext, call_tool_on_client, connect_with_context, get_prompt_on_client,
    list_prompts_on_client, list_resources_on_client, list_tools_on_client, read_resource_on_client, shutdown_client,
};
use super::config::McpServerConfig;
use super::events::{McpEventBus, McpServerEvent};

/// One connected MCP server with exclusive client access.
pub struct McpServerSession {
    name: String,
    config: McpServerConfig,
    auth_store_path: Option<PathBuf>,
    events: Option<mpsc::UnboundedSender<McpServerEvent>>,
    client: Mutex<Option<McpClient>>,
}

impl McpServerSession {
    pub fn new(name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            auth_store_path: None,
            events: None,
            client: Mutex::new(None),
        }
    }

    pub fn with_auth_store_path(mut self, auth_store_path: Option<PathBuf>) -> Self {
        self.auth_store_path = auth_store_path;
        self
    }

    pub fn with_events(mut self, events: Option<mpsc::UnboundedSender<McpServerEvent>>) -> Self {
        self.events = events;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    fn connect_ctx(&self) -> McpConnectContext {
        let mut ctx = McpConnectContext::named(&self.name);
        if let Some(path) = &self.auth_store_path {
            ctx = ctx.with_auth_store_path(path.clone());
        }
        if let Some(tx) = &self.events {
            ctx = ctx.with_events(tx.clone());
        }
        ctx
    }

    async fn ensure_client_locked(
        guard: &mut Option<McpClient>,
        name: &str,
        config: &McpServerConfig,
        ctx: &McpConnectContext,
    ) -> Result<()> {
        if guard.is_some() {
            return Ok(());
        }
        log::debug!("opening MCP session: server={name}");
        let client = connect_with_context(config, ctx)
            .await
            .with_context(|| format!("connect MCP server \"{name}\""))?;
        *guard = Some(client);
        Ok(())
    }

    async fn drop_client_locked(guard: &mut Option<McpClient>) {
        if let Some(client) = guard.take() {
            shutdown_client(client).await;
        }
    }

    /// Drop the current client (if any) and shut it down.
    pub async fn drop_client(&self) {
        let mut guard = self.client.lock().await;
        Self::drop_client_locked(&mut guard).await;
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.list_tools_inner())
            .await
            .with_context(|| format!("list tools on \"{}\" timed out after {op_timeout:?}", self.name))?
    }

    async fn list_tools_inner(&self) -> Result<Vec<Tool>> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match list_tools_on_client(client).await {
            Ok(tools) => Ok(tools),
            Err(first_error) => {
                log::warn!(
                    "MCP list_tools failed; reconnecting once: server={} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                list_tools_on_client(client)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    pub async fn list_resources(&self) -> Result<Vec<Resource>> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.list_resources_inner())
            .await
            .with_context(|| format!("list resources on \"{}\" timed out", self.name))?
    }

    async fn list_resources_inner(&self) -> Result<Vec<Resource>> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match list_resources_on_client(client).await {
            Ok(v) => Ok(v),
            Err(first_error) => {
                log::warn!(
                    "MCP list_resources failed; reconnecting once: server={} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                list_resources_on_client(client)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    pub async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.list_prompts_inner())
            .await
            .with_context(|| format!("list prompts on \"{}\" timed out", self.name))?
    }

    async fn list_prompts_inner(&self) -> Result<Vec<Prompt>> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match list_prompts_on_client(client).await {
            Ok(v) => Ok(v),
            Err(first_error) => {
                log::warn!(
                    "MCP list_prompts failed; reconnecting once: server={} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                list_prompts_on_client(client)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContents>> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.read_resource_inner(uri))
            .await
            .with_context(|| format!("read resource on \"{}\" timed out", self.name))?
    }

    async fn read_resource_inner(&self, uri: &str) -> Result<Vec<ResourceContents>> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match read_resource_on_client(client, uri).await {
            Ok(v) => Ok(v),
            Err(first_error) => {
                log::warn!(
                    "MCP read_resource failed; reconnecting once: server={} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                read_resource_on_client(client, uri)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    pub async fn get_prompt(&self, name: &str, arguments: Option<Value>) -> Result<GetPromptResult> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.get_prompt_inner(name, arguments))
            .await
            .with_context(|| format!("get prompt on \"{}\" timed out", self.name))?
    }

    async fn get_prompt_inner(&self, name: &str, arguments: Option<Value>) -> Result<GetPromptResult> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match get_prompt_on_client(client, name, arguments.clone()).await {
            Ok(v) => Ok(v),
            Err(first_error) => {
                log::warn!(
                    "MCP get_prompt failed; reconnecting once: server={} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                get_prompt_on_client(client, name, arguments)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    pub async fn call_tool(&self, tool_name: &str, args: Value) -> Result<CallToolResult> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.call_tool_inner(tool_name, args))
            .await
            .with_context(|| format!("call tool on \"{}\" timed out after {op_timeout:?}", self.name))?
    }

    async fn call_tool_inner(&self, tool_name: &str, args: Value) -> Result<CallToolResult> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match call_tool_on_client(client, tool_name, args.clone()).await {
            Ok(result) => Ok(result),
            Err(first_error) => {
                log::warn!(
                    "MCP call_tool failed; reconnecting once: server={} tool={tool_name} error={first_error}",
                    self.name
                );
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                call_tool_on_client(client, tool_name, args)
                    .await
                    .with_context(|| format!("MCP server \"{}\" after reconnect: {first_error}", self.name))
            }
        }
    }

    /// Close the session (best-effort).
    pub async fn close(&self) {
        self.drop_client().await;
    }
}

impl Drop for McpServerSession {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.client.try_lock()
            && let Some(client) = guard.take()
        {
            tokio::spawn(async move {
                shutdown_client(client).await;
            });
        }
    }
}

/// Pool of named MCP server sessions.
pub struct McpSessionPool {
    sessions: Mutex<HashMap<String, Arc<McpServerSession>>>,
    auth_store_path: Option<PathBuf>,
    event_bus: McpEventBus,
}

impl Default for McpSessionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSessionPool {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            auth_store_path: None,
            event_bus: McpEventBus::new(),
        }
    }

    pub fn with_auth_store_path(mut self, auth_store_path: Option<PathBuf>) -> Self {
        self.auth_store_path = auth_store_path;
        self
    }

    pub fn event_bus(&self) -> &McpEventBus {
        &self.event_bus
    }

    pub fn set_event_sender(&self, tx: mpsc::UnboundedSender<McpServerEvent>) {
        self.event_bus.set_sender(tx);
    }

    /// Get or create a session for `name` with the given config.
    pub async fn get_or_insert(&self, name: &str, config: McpServerConfig) -> Arc<McpServerSession> {
        let old = {
            let mut sessions = self.sessions.lock().await;
            if let Some(existing) = sessions.get(name)
                && existing.config() == &config
            {
                return Arc::clone(existing);
            }
            sessions.remove(name)
        };
        if let Some(old) = old {
            old.close().await;
        }
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(name)
            && existing.config() == &config
        {
            return Arc::clone(existing);
        }
        let session = Arc::new(
            McpServerSession::new(name, config)
                .with_auth_store_path(self.auth_store_path.clone())
                .with_events(self.event_bus.sender()),
        );
        sessions.insert(name.to_string(), Arc::clone(&session));
        session
    }

    pub async fn get(&self, name: &str) -> Option<Arc<McpServerSession>> {
        self.sessions.lock().await.get(name).cloned()
    }

    pub async fn remove(&self, name: &str) -> bool {
        let session = self.sessions.lock().await.remove(name);
        if let Some(session) = session {
            session.close().await;
            true
        } else {
            false
        }
    }

    pub async fn close_all(&self) {
        let sessions: Vec<_> = {
            let mut guard = self.sessions.lock().await;
            guard.drain().map(|(_, s)| s).collect()
        };
        for session in sessions {
            session.close().await;
        }
    }

    pub async fn len(&self) -> usize {
        self.sessions.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.sessions.lock().await.is_empty()
    }

    pub async fn list_tools(&self, name: &str, config: McpServerConfig) -> Result<Vec<Tool>> {
        let session = self.get_or_insert(name, config).await;
        session.list_tools().await
    }

    pub async fn list_resources(&self, name: &str, config: McpServerConfig) -> Result<Vec<Resource>> {
        let session = self.get_or_insert(name, config).await;
        session.list_resources().await
    }

    pub async fn list_prompts(&self, name: &str, config: McpServerConfig) -> Result<Vec<Prompt>> {
        let session = self.get_or_insert(name, config).await;
        session.list_prompts().await
    }

    pub async fn read_resource(&self, name: &str, config: McpServerConfig, uri: &str) -> Result<Vec<ResourceContents>> {
        let session = self.get_or_insert(name, config).await;
        session.read_resource(uri).await
    }

    pub async fn get_prompt(
        &self,
        name: &str,
        config: McpServerConfig,
        prompt_name: &str,
        arguments: Option<Value>,
    ) -> Result<GetPromptResult> {
        let session = self.get_or_insert(name, config).await;
        session.get_prompt(prompt_name, arguments).await
    }

    pub async fn call_tool(
        &self,
        name: &str,
        config: McpServerConfig,
        tool_name: &str,
        args: Value,
    ) -> Result<CallToolResult> {
        if config.is_disabled() {
            bail!("MCP server \"{name}\" is disabled");
        }
        let session = self.get_or_insert(name, config).await;
        session.call_tool(tool_name, args).await
    }
}

impl Drop for McpSessionPool {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.sessions.try_lock() {
            guard.clear();
        }
    }
}
