//! Long-lived MCP client sessions with reconnect-on-failure.
//!
//! Production tool calls reuse these sessions instead of spawning a new
//! stdio process (or HTTP handshake) for every invocation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, Tool};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::cache::{McpCacheStore, is_read_only_tool};
use super::client::call_tool_on_client;
use super::client::cancel_task_on_client;
use super::client::connect_with_context;
use super::client::get_prompt_on_client;
use super::client::get_task_on_client;
use super::client::list_prompts_on_client;
use super::client::list_resources_on_client;
use super::client::list_tools_on_client;
use super::client::peer_supports_tasks;
use super::client::read_resource_on_client;
use super::client::shutdown_client;
use super::client::update_task_on_client;
use super::client::{McpClient, McpConnectContext};
use super::config::{McpResponseCacheConfig, McpServerConfig};
use super::events::{McpEventBus, McpServerEvent};

/// One connected MCP server with exclusive client access.
pub struct McpServerSession {
    name: String,
    config: McpServerConfig,
    auth_store_path: Option<PathBuf>,
    events: Option<mpsc::UnboundedSender<McpServerEvent>>,
    response_cache: McpResponseCacheConfig,
    client: Mutex<Option<McpClient>>,
}

impl McpServerSession {
    pub fn new(name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            auth_store_path: None,
            events: None,
            response_cache: McpResponseCacheConfig::default(),
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

    pub fn with_response_cache(mut self, response_cache: McpResponseCacheConfig) -> Self {
        self.response_cache = response_cache;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    fn connect_ctx(&self) -> McpConnectContext {
        // Re-resolve OAuth on every connect/reconnect (SSE bearer + HTTP AuthClient store).
        let mut ctx = McpConnectContext::named(&self.name)
            .with_mrtr_elicitation(self.config.mrtr_elicitation())
            .with_response_cache(self.response_cache.clone());
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
        let op_timeout = self.config.operation_timeout();
        let ctx = self.connect_ctx();
        let tool_name = tool_name.to_string();

        let max_attempts = 3;
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 1..=max_attempts {
            if attempt > 1 {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt as u32 - 2));
                log::debug!(
                    "MCP retry: server={} tool={tool_name} attempt={attempt}/{max_attempts} backoff={delay:?}",
                    self.name
                );
                tokio::time::sleep(delay).await;
            }

            // Scope the lock guard so it's released between retries
            let result = {
                let mut guard = self.client.lock().await;

                // Drop and reconnect on retry
                if attempt > 1 {
                    log::warn!(
                        "MCP call_tool reconnecting: server={} tool={tool_name} attempt={attempt}",
                        self.name
                    );
                    Self::drop_client_locked(&mut guard).await;
                }

                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing")?;

                match tokio::time::timeout(op_timeout, call_tool_on_client(client, &tool_name, args.clone())).await {
                    Ok(Ok(result)) => return Ok(result),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(anyhow::anyhow!("timed out after {op_timeout:?}")),
                }
            };

            // Use shared retryable classification from elph-ai
            let err = result.as_ref().unwrap_err();
            if elph_ai::resilience::retry::is_anyhow_retryable(err) {
                last_err = result.err();
                log::warn!(
                    "MCP call_tool failed; retrying: server={} tool={tool_name} attempt={attempt} error={}",
                    self.name,
                    last_err.as_ref().unwrap(),
                );
                continue;
            }

            // Non-retryable — fail immediately
            return result;
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!(
                "MCP call_tool failed after {max_attempts} attempts: server={} tool={tool_name}",
                self.name
            )
        }))
    }

    /// Close the session (best-effort).
    pub async fn close(&self) {
        self.drop_client().await;
    }

    /// Whether the peer currently advertises Tasks (requires an open client).
    pub async fn supports_tasks(&self) -> Result<bool> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        Ok(peer_supports_tasks(client))
    }

    pub async fn get_task(&self, task_id: &str) -> Result<rmcp::model::GetTaskResult> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.get_task_inner(task_id))
            .await
            .with_context(|| format!("tasks/get on \"{}\" timed out", self.name))?
    }

    async fn get_task_inner(&self, task_id: &str) -> Result<rmcp::model::GetTaskResult> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        match get_task_on_client(client, task_id).await {
            Ok(r) => Ok(r),
            Err(first) => {
                log::warn!("MCP tasks/get failed; reconnecting: server={} error={first}", self.name);
                Self::drop_client_locked(&mut guard).await;
                Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
                let client = guard.as_ref().context("MCP client missing after reconnect")?;
                get_task_on_client(client, task_id).await
            }
        }
    }

    pub async fn update_task(&self, task_id: &str, input_responses: rmcp::model::InputResponses) -> Result<()> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.update_task_inner(task_id, input_responses))
            .await
            .with_context(|| format!("tasks/update on \"{}\" timed out", self.name))?
    }

    async fn update_task_inner(&self, task_id: &str, input_responses: rmcp::model::InputResponses) -> Result<()> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        update_task_on_client(client, task_id, input_responses).await
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        let op_timeout = self.config.operation_timeout();
        timeout(op_timeout, self.cancel_task_inner(task_id))
            .await
            .with_context(|| format!("tasks/cancel on \"{}\" timed out", self.name))?
    }

    async fn cancel_task_inner(&self, task_id: &str) -> Result<()> {
        let ctx = self.connect_ctx();
        let mut guard = self.client.lock().await;
        Self::ensure_client_locked(&mut guard, &self.name, &self.config, &ctx).await?;
        let client = guard.as_ref().context("MCP client missing")?;
        cancel_task_on_client(client, task_id).await
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
    response_cache: McpResponseCacheConfig,
    event_bus: McpEventBus,
    cache_store: Option<Arc<McpCacheStore>>,
    default_cache_ttl_ms: u64,
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
            response_cache: McpResponseCacheConfig::default(),
            event_bus: McpEventBus::new(),
            cache_store: None,
            default_cache_ttl_ms: super::cache::DEFAULT_CACHE_TTL_MS,
        }
    }

    pub fn with_auth_store_path(mut self, auth_store_path: Option<PathBuf>) -> Self {
        self.auth_store_path = auth_store_path;
        self
    }

    pub fn with_response_cache(mut self, response_cache: McpResponseCacheConfig) -> Self {
        self.response_cache = response_cache;
        self
    }

    pub fn with_cache_store(mut self, cache_store: Option<Arc<McpCacheStore>>) -> Self {
        self.cache_store = cache_store;
        self
    }

    /// Set the default tool result cache TTL (ms) used when a server does not
    /// override it via `cacheTtlMs`. `0` disables caching.
    pub fn with_default_cache_ttl(mut self, ttl_ms: u64) -> Self {
        self.default_cache_ttl_ms = ttl_ms;
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
                .with_events(self.event_bus.sender())
                .with_response_cache(self.response_cache.clone()),
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

        // Cache hit: return immediately for read-only tools.
        if is_read_only_tool(tool_name)
            && let Some(cache) = &self.cache_store
            && let Some(cached) = cache.get(name, tool_name, &args)
        {
            return Ok(cached);
        }

        let ttl = config.cache_ttl_ms().unwrap_or(self.default_cache_ttl_ms);
        let session = self.get_or_insert(name, config).await;
        let result = session.call_tool(tool_name, args.clone()).await?;

        // Cache miss: store result for read-only tools.
        if is_read_only_tool(tool_name)
            && let Some(cache) = &self.cache_store
        {
            let _ = cache.set(name, tool_name, &args, &result, ttl);
        }

        Ok(result)
    }

    pub async fn get_task(
        &self,
        name: &str,
        config: McpServerConfig,
        task_id: &str,
    ) -> Result<rmcp::model::GetTaskResult> {
        let session = self.get_or_insert(name, config).await;
        session.get_task(task_id).await
    }

    pub async fn update_task(
        &self,
        name: &str,
        config: McpServerConfig,
        task_id: &str,
        input_responses: rmcp::model::InputResponses,
    ) -> Result<()> {
        let session = self.get_or_insert(name, config).await;
        session.update_task(task_id, input_responses).await
    }

    pub async fn cancel_task(&self, name: &str, config: McpServerConfig, task_id: &str) -> Result<()> {
        let session = self.get_or_insert(name, config).await;
        session.cancel_task(task_id).await
    }
}

impl Drop for McpSessionPool {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.sessions.try_lock() {
            guard.clear();
        }
    }
}
