//! MCP client connectivity via rmcp (stdio + streamable HTTP + SSE + OAuth).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::bail;
use anyhow::{Context, Result};
use http::{HeaderName, HeaderValue};
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResponse;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::GetPromptRequestParams;
use rmcp::model::Prompt;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::Resource;
use rmcp::model::ResourceContents;
use rmcp::model::Tool;
use rmcp::service::ClientCacheConfig;
use rmcp::service::RunningService;
use rmcp::{ClientLifecycleMode, ClientServiceExt};

use super::config::{McpLifecycleMode, McpMrtrElicitationPolicy, McpResponseCacheConfig};

/// Resolve the rmcp `ClientLifecycleMode` from elph's config-level mode.
///
/// `Auto` → probe `server/discover`, fall back to `initialize` on legacy.
/// `Legacy` → always use the `initialize` / `notifications/initialized` handshake.
/// `Discover` → use `server/discover` exclusively.
fn resolve_lifecycle(elph_mode: McpLifecycleMode) -> ClientLifecycleMode {
    match elph_mode {
        McpLifecycleMode::Auto => ClientLifecycleMode::Auto {
            preferred_versions: vec![
                rmcp::model::ProtocolVersion::V_2026_07_28,
                rmcp::model::ProtocolVersion::V_2025_11_25,
            ],
            legacy_version: Some(rmcp::model::ProtocolVersion::V_2025_11_25),
        },
        McpLifecycleMode::Legacy => ClientLifecycleMode::Initialize,
        McpLifecycleMode::Discover => ClientLifecycleMode::Discover {
            preferred_versions: vec![
                rmcp::model::ProtocolVersion::V_2026_07_28,
                rmcp::model::ProtocolVersion::V_2025_11_25,
            ],
        },
    }
}

use rmcp::transport::auth::AuthClient;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::auth::authorization_manager_from_store;
use super::auth_resolve::ResolvedMcpAuth;
use super::auth_resolve::resolve_remote_auth;
use super::compat::resolve_http_headers;
use super::config::{McpHttpConfig, McpServerConfig, McpStdioConfig};
use super::events::{McpClientService, McpServerEvent};
use super::sse::SseClientTransport;

/// Live MCP client service (stdio process, HTTP session, or SSE).
pub type McpClient = RunningService<rmcp::RoleClient, McpClientService>;

/// Default probe/list timeout used by CLI doctor when no server timeout is set.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Context for opening a connection (name, auth store, notifications).
#[derive(Debug, Clone, Default)]
pub struct McpConnectContext {
    pub server_name: String,
    /// Full path to shared `auth.json` (or host-chosen name). See [`super::auth::AuthStorePathBuilder`].
    pub auth_store_path: Option<PathBuf>,
    pub events: Option<mpsc::UnboundedSender<McpServerEvent>>,
    /// SEP-2549 client list-response cache.
    pub response_cache: McpResponseCacheConfig,
    /// SEP-2322 MRTR elicitation policy.
    pub mrtr_elicitation: McpMrtrElicitationPolicy,
}

impl McpConnectContext {
    pub fn named(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            auth_store_path: None,
            events: None,
            response_cache: McpResponseCacheConfig::default(),
            mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
        }
    }

    /// Full path to the credential store file.
    pub fn with_auth_store_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.auth_store_path = Some(path.into());
        self
    }

    pub fn with_events(mut self, tx: mpsc::UnboundedSender<McpServerEvent>) -> Self {
        self.events = Some(tx);
        self
    }

    pub fn with_response_cache(mut self, cache: McpResponseCacheConfig) -> Self {
        self.response_cache = cache;
        self
    }

    pub fn with_mrtr_elicitation(mut self, policy: McpMrtrElicitationPolicy) -> Self {
        self.mrtr_elicitation = policy;
        self
    }
}

#[derive(Debug, Clone)]
pub struct McpProbeResult {
    pub name: String,
    pub ok: bool,
    pub tool_count: usize,
    pub transport: String,
    pub message: String,
    /// Negotiated protocol version string when known (e.g. `2026-07-28`).
    pub protocol_version: Option<String>,
    /// Configured lifecycle mode label (`auto` / `legacy` / `discover`).
    pub lifecycle: String,
}

fn lifecycle_label(mode: McpLifecycleMode) -> &'static str {
    match mode {
        McpLifecycleMode::Auto => "auto",
        McpLifecycleMode::Legacy => "legacy",
        McpLifecycleMode::Discover => "discover",
    }
}

fn handler_for_ctx(ctx: &McpConnectContext) -> McpClientService {
    McpClientService::new(&ctx.server_name, ctx.events.clone()).with_mrtr_elicitation(ctx.mrtr_elicitation)
}

async fn apply_response_cache(client: &McpClient, cache: &McpResponseCacheConfig) {
    let mut cfg = if cache.enabled {
        ClientCacheConfig::default()
    } else {
        ClientCacheConfig::disabled()
    };
    if let Some(ms) = cache.default_ttl_ms {
        cfg = cfg.with_default_ttl(Duration::from_millis(ms));
    }
    if let Some(part) = &cache.private_partition {
        cfg = cfg.with_private_partition(part.clone());
    }
    client.set_response_cache_config(cfg).await;
}

fn peer_protocol_version(client: &McpClient) -> Option<String> {
    client
        .peer_info()
        .map(|info| info.protocol_version.as_str().to_string())
}

/// Whether the peer advertised the Tasks extension (SEP-2663).
pub fn peer_supports_tasks(client: &McpClient) -> bool {
    client
        .peer_info()
        .map(|info| info.capabilities.supports_tasks())
        .unwrap_or(false)
}

/// Open a long-lived connection (caller owns lifecycle / cancel).
pub async fn connect(config: &McpServerConfig) -> Result<McpClient> {
    connect_with_context(config, &McpConnectContext::default()).await
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.mcp.connect"))]
pub async fn connect_with_context(config: &McpServerConfig, ctx: &McpConnectContext) -> Result<McpClient> {
    // The rmcp client writes transport/connection diagnostics directly to fd 2,
    // bypassing the `log` crate. Redirect those to a log file so they stay out
    // of the TUI (mirrors the browser backend's stderr suppression).
    crate::logger::redirect_stderr_to_file();
    let lifecycle = resolve_lifecycle(config.lifecycle_mode());
    let result = match config {
        McpServerConfig::Stdio(cfg) => connect_stdio_with_context(cfg, ctx, lifecycle.clone()).await,
        McpServerConfig::Http(cfg) => connect_http_with_context(cfg, ctx, lifecycle.clone()).await,
        McpServerConfig::Sse(cfg) => connect_sse_with_context(cfg, ctx, lifecycle.clone()).await,
    };

    // Auto mode resilience: some servers (e.g. DeepWiki) reject `server/discover`
    // with non-standard error codes instead of `METHOD_NOT_FOUND`. Fall back to
    // the legacy handshake when Auto mode fails.
    if matches!(lifecycle, ClientLifecycleMode::Auto { .. })
        && let Err(ref err) = result
    {
        log::warn!(
            "MCP Auto mode failed for \"{}\", falling back to Initialize: {err}",
            ctx.server_name
        );
        return match config {
            McpServerConfig::Stdio(cfg) => connect_stdio_with_context(cfg, ctx, ClientLifecycleMode::Initialize).await,
            McpServerConfig::Http(cfg) => connect_http_with_context(cfg, ctx, ClientLifecycleMode::Initialize).await,
            McpServerConfig::Sse(cfg) => connect_sse_with_context(cfg, ctx, ClientLifecycleMode::Initialize).await,
        };
    }

    result
}

pub async fn connect_stdio(config: &McpStdioConfig) -> Result<McpClient> {
    connect_stdio_with_context(config, &McpConnectContext::default(), ClientLifecycleMode::Initialize).await
}

pub async fn connect_stdio_with_context(
    config: &McpStdioConfig,
    ctx: &McpConnectContext,
    lifecycle: ClientLifecycleMode,
) -> Result<McpClient> {
    let mut command = Command::new(&config.command);
    command.args(&config.args);
    command.envs(config.env.iter());
    if let Some(cwd) = &config.cwd {
        command.current_dir(cwd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    // Capture stderr for debugging failed servers; do not inherit to avoid TUI noise.
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);

    log::debug!("spawning MCP stdio server: command={} args={:?}", config.command, config.args);
    let transport = TokioChildProcess::new(command.configure(|_| {})).context("spawn MCP stdio transport")?;
    let handler = handler_for_ctx(ctx);
    let client = handler
        .serve_with_lifecycle(transport, lifecycle)
        .await
        .context("initialize MCP stdio client")?;
    apply_response_cache(&client, &ctx.response_cache).await;
    Ok(client)
}

pub async fn connect_http(config: &McpHttpConfig) -> Result<McpClient> {
    connect_http_with_context(config, &McpConnectContext::default(), ClientLifecycleMode::Initialize).await
}

pub async fn connect_http_with_context(
    config: &McpHttpConfig,
    ctx: &McpConnectContext,
    lifecycle: ClientLifecycleMode,
) -> Result<McpClient> {
    let handler = handler_for_ctx(ctx);
    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(config.url.clone());

    // Custom headers always applied.
    if !config.headers.is_empty() {
        let resolved = resolve_http_headers(&config.headers)?;
        let mut headers = std::collections::HashMap::new();
        for (key, value) in &resolved {
            let name =
                HeaderName::from_bytes(key.as_bytes()).with_context(|| format!("invalid HTTP header name: {key}"))?;
            let value = HeaderValue::from_str(value).with_context(|| format!("invalid HTTP header value for {key}"))?;
            headers.insert(name, value);
        }
        transport_config = transport_config.custom_headers(headers);
    }

    if config.oauth && ctx.auth_store_path.is_none() {
        bail!(
            "MCP server \"{}\" requires OAuth but no auth store path was configured",
            ctx.server_name
        );
    }

    let resolved = resolve_remote_auth(config, &ctx.server_name, ctx.auth_store_path.as_deref()).await?;
    log::debug!(
        "connecting MCP HTTP: url={} server={} auth={}",
        config.url,
        ctx.server_name,
        resolved.source_label()
    );

    match resolved {
        ResolvedMcpAuth::Oauth { .. } => {
            let store_path = ctx
                .auth_store_path
                .as_ref()
                .context("oauth resolved without auth store path")?;
            let manager = authorization_manager_from_store(&config.url, store_path, &ctx.server_name)
                .await?
                .context("oauth credentials missing after resolve")?;
            let http = reqwest::Client::builder()
                .pool_max_idle_per_host(0)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .context("build OAuth HTTP client")?;
            // AuthClient injects refreshed Bearer; do not also set static auth_header.
            let auth_client = AuthClient::new(http, manager);
            let transport = StreamableHttpClientTransport::with_client(auth_client, transport_config);
            let client = handler
                .serve_with_lifecycle(transport, lifecycle)
                .await
                .context("initialize MCP HTTP OAuth client")?;
            apply_response_cache(&client, &ctx.response_cache).await;
            Ok(client)
        }
        ResolvedMcpAuth::StaticBearer { token, source } => {
            log::debug!("MCP HTTP using static bearer: source={source:?}");
            transport_config = transport_config.auth_header(token);
            let transport = StreamableHttpClientTransport::from_config(transport_config);
            let client = handler
                .serve_with_lifecycle(transport, lifecycle.clone())
                .await
                .context("initialize MCP HTTP client")?;
            apply_response_cache(&client, &ctx.response_cache).await;
            Ok(client)
        }
        ResolvedMcpAuth::None => {
            let transport = StreamableHttpClientTransport::from_config(transport_config);
            let client = handler
                .serve_with_lifecycle(transport, lifecycle)
                .await
                .context("initialize MCP HTTP client")?;
            apply_response_cache(&client, &ctx.response_cache).await;
            Ok(client)
        }
    }
}

pub async fn connect_sse_with_context(
    config: &McpHttpConfig,
    ctx: &McpConnectContext,
    lifecycle: ClientLifecycleMode,
) -> Result<McpClient> {
    log::debug!("connecting MCP SSE server: url={}", config.url);

    if config.oauth && ctx.auth_store_path.is_none() {
        bail!(
            "MCP server \"{}\" requires OAuth but no auth store path was configured",
            ctx.server_name
        );
    }

    let resolved = resolve_remote_auth(config, &ctx.server_name, ctx.auth_store_path.as_deref()).await?;
    log::debug!(
        "MCP SSE auth resolved: server={} auth={}",
        ctx.server_name,
        resolved.source_label()
    );
    let bearer = resolved.bearer_token().map(str::to_string);

    if config.oauth {
        log::warn!(
            "MCP SSE + OAuth: transport is deprecated (prefer type=http); token refreshed on reconnect only: server={}",
            ctx.server_name
        );
    }

    let transport = SseClientTransport::connect_with_bearer(config, bearer)
        .await
        .with_context(|| format!("connect SSE MCP at {}", config.url))?;
    let handler = handler_for_ctx(ctx);
    let client = handler
        .serve_with_lifecycle(transport, lifecycle)
        .await
        .context("initialize MCP SSE client")?;
    apply_response_cache(&client, &ctx.response_cache).await;
    Ok(client)
}

pub async fn list_tools_on_client(client: &McpClient) -> Result<Vec<Tool>> {
    client.list_all_tools().await.context("list MCP tools")
}

pub async fn list_resources_on_client(client: &McpClient) -> Result<Vec<Resource>> {
    client.list_all_resources().await.context("list MCP resources")
}

pub async fn list_prompts_on_client(client: &McpClient) -> Result<Vec<Prompt>> {
    client.list_all_prompts().await.context("list MCP prompts")
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.mcp.call_tool"))]
pub async fn call_tool_on_client(client: &McpClient, tool_name: &str, args: Value) -> Result<CallToolResult> {
    let mut params = CallToolRequestParams::new(tool_name.to_string());
    if let Value::Object(map) = args {
        params = params.with_arguments(map);
    }
    // Use call_tool_once (non-MRTR) which works with all transports (stdio, HTTP, SSE).
    // The MRTR-aware call_tool() requires HTTP transport and fails on stdio with
    // "Requires HTTP transport (--port)". Since we don't support interactive MRTR
    // rounds in the agent harness (MrtrElicitationPolicy::Decline), call_tool_once
    // is the correct choice for all transports.
    match client.call_tool_once(params).await.context("call MCP tool")? {
        CallToolResponse::Complete(result) => Ok(result),
        CallToolResponse::Task(task) => {
            let payload = serde_json::json!({
                "resultType": "task",
                "taskId": task.task.task_id,
                "status": format!("{:?}", task.task.status),
                "statusMessage": task.task.status_message,
                "pollIntervalMs": task.task.poll_interval_ms,
                "hint": "Poll with mcp_{server}__tasks_get using taskId",
            });
            let mut result = CallToolResult::success(vec![ContentBlock::text(payload.to_string())]);
            result.structured_content = Some(payload);
            Ok(result)
        }
        CallToolResponse::InputRequired(_) => Err(anyhow::anyhow!(
            "MCP tool returned input_required; use HTTP transport for interactive MRTR rounds: {tool_name}"
        )),
        _ => Err(anyhow::anyhow!(
            "MCP tool returned an unexpected call response shape: {tool_name}"
        )),
    }
}

pub async fn read_resource_on_client(client: &McpClient, uri: &str) -> Result<Vec<ResourceContents>> {
    let params = ReadResourceRequestParams::new(uri.to_string());
    let result = client.read_resource(params).await.context("read MCP resource")?;
    Ok(result.contents)
}

pub async fn get_prompt_on_client(
    client: &McpClient,
    name: &str,
    arguments: Option<Value>,
) -> Result<rmcp::model::GetPromptResult> {
    let mut params = GetPromptRequestParams::new(name.to_string());
    if let Some(Value::Object(map)) = arguments {
        params = params.with_arguments(map);
    }
    client.get_prompt(params).await.context("get MCP prompt")
}

/// Gracefully cancel a client; errors are logged.
pub async fn shutdown_client(client: McpClient) {
    if let Err(error) = client.cancel().await {
        log::warn!("MCP client shutdown error: {error}");
    }
}

pub async fn probe_stdio_server(name: &str, config: &McpStdioConfig) -> McpProbeResult {
    probe_server(name, &McpServerConfig::Stdio(config.clone())).await
}

pub async fn probe_server(name: &str, config: &McpServerConfig) -> McpProbeResult {
    probe_server_with_auth(name, config, None).await
}

pub async fn probe_server_with_auth(
    name: &str,
    config: &McpServerConfig,
    auth_store_path: Option<&Path>,
) -> McpProbeResult {
    let transport = config.kind_label().to_string();
    let lifecycle = lifecycle_label(config.lifecycle_mode()).to_string();
    let op_timeout = config.operation_timeout();
    let mut ctx = McpConnectContext::named(name).with_mrtr_elicitation(config.mrtr_elicitation());
    if let Some(path) = auth_store_path {
        ctx = ctx.with_auth_store_path(path);
    }
    match timeout(op_timeout, async {
        let client = connect_with_context(config, &ctx).await?;
        let protocol_version = peer_protocol_version(&client);
        let tasks = peer_supports_tasks(&client);
        let tools = list_tools_on_client(&client).await.unwrap_or_default();
        let count = tools.len();
        shutdown_client(client).await;
        Ok::<_, anyhow::Error>((count, protocol_version, tasks))
    })
    .await
    {
        Ok(Ok((count, protocol_version, tasks))) => {
            let proto = protocol_version.as_deref().unwrap_or("unknown");
            let mut message = format!("connected, {count} tools, protocol={proto}, lifecycle={lifecycle}");
            if tasks {
                message.push_str(", tasks=yes");
            }
            if config.is_sse() {
                message.push_str(" (sse deprecated; prefer type=http)");
            }
            McpProbeResult {
                name: name.to_string(),
                ok: true,
                tool_count: count,
                transport,
                message,
                protocol_version,
                lifecycle,
            }
        }
        Ok(Err(error)) => McpProbeResult {
            name: name.to_string(),
            ok: false,
            tool_count: 0,
            transport,
            message: error.to_string(),
            protocol_version: None,
            lifecycle,
        },
        Err(_) => McpProbeResult {
            name: name.to_string(),
            ok: false,
            tool_count: 0,
            transport,
            message: format!("timed out after {op_timeout:?}"),
            protocol_version: None,
            lifecycle,
        },
    }
}

/// Task helpers (SEP-2663) on a live client.
pub async fn get_task_on_client(client: &McpClient, task_id: &str) -> Result<rmcp::model::GetTaskResult> {
    client
        .get_task(rmcp::model::GetTaskParams::new(task_id.to_string()))
        .await
        .context("tasks/get")
}

pub async fn update_task_on_client(
    client: &McpClient,
    task_id: &str,
    input_responses: rmcp::model::InputResponses,
) -> Result<()> {
    client
        .update_task(rmcp::model::UpdateTaskParams::new(task_id.to_string(), input_responses))
        .await
        .context("tasks/update")
}

pub async fn cancel_task_on_client(client: &McpClient, task_id: &str) -> Result<()> {
    client
        .cancel_task(rmcp::model::CancelTaskParams::new(task_id.to_string()))
        .await
        .context("tasks/cancel")
}

/// One-shot list tools (connects, lists, disconnects). Prefer session pool for repeated calls.
pub async fn list_tools(config: &McpStdioConfig) -> Result<Vec<Tool>> {
    list_tools_for_server(&McpServerConfig::Stdio(config.clone())).await
}

pub async fn list_tools_for_server(config: &McpServerConfig) -> Result<Vec<Tool>> {
    let op_timeout = config.operation_timeout();
    timeout(op_timeout, async {
        let client = connect(config).await?;
        let tools = list_tools_on_client(&client).await?;
        shutdown_client(client).await;
        Ok(tools)
    })
    .await
    .context("MCP list tools timed out")?
}

/// One-shot tool call (connects, calls, disconnects). Prefer session pool for production.
pub async fn call_stdio_tool(config: &McpStdioConfig, tool_name: &str, args: Value) -> Result<CallToolResult> {
    call_tool_for_server(&McpServerConfig::Stdio(config.clone()), tool_name, args).await
}

pub async fn call_tool_for_server(config: &McpServerConfig, tool_name: &str, args: Value) -> Result<CallToolResult> {
    let op_timeout = config.operation_timeout();
    timeout(op_timeout, async {
        let client = connect(config).await?;
        let result = call_tool_on_client(&client, tool_name, args).await?;
        shutdown_client(client).await;
        Ok(result)
    })
    .await
    .context("MCP tool call timed out")?
}

/// Build a stdio config from CLI pieces.
pub fn parse_stdio_config(command: String, args: Vec<String>, env: BTreeMap<String, String>) -> McpStdioConfig {
    McpStdioConfig {
        command,
        args,
        env,
        cwd: None,
        timeout_ms: None,
        enable: true,
        lifecycle: Default::default(),
        mrtr_elicitation: Default::default(),
        policy: None,
        load_strategy: Default::default(),
        cache_ttl_ms: None,
    }
}

/// Validate a server config before connecting.
pub fn validate_server_config(name: &str, config: &McpServerConfig) -> Result<()> {
    if name.trim().is_empty() {
        bail!("MCP server name must not be empty");
    }
    match config {
        McpServerConfig::Stdio(cfg) => {
            if cfg.command.trim().is_empty() {
                bail!("MCP server \"{name}\" stdio command must not be empty");
            }
        }
        McpServerConfig::Http(cfg) | McpServerConfig::Sse(cfg) => {
            if cfg.url.trim().is_empty() {
                bail!("MCP server \"{name}\" url must not be empty");
            }
            let parsed = url::Url::parse(&cfg.url).with_context(|| format!("MCP server \"{name}\" invalid url"))?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                bail!("MCP server \"{name}\" url must be http or https");
            }
        }
    }
    Ok(())
}
