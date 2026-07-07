use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::websocket_connect::{WsStream, connect_websocket_with_proxy};
use crate::types::ProviderEnv;

use crate::api::common::send_with_abort;
use crate::api::sse::for_each_sse_json_event;
use tokio_util::sync::CancellationToken;

const OPENAI_BETA_RESPONSES_WEBSOCKETS: &str = "responses_websockets=2026-02-06";
const WEBSOCKET_CONNECTION_LIMIT_REACHED: &str = "websocket_connection_limit_reached";
const SESSION_WEBSOCKET_CACHE_TTL_MS: u64 = 5 * 60 * 1000;
const SESSION_WEBSOCKET_MAX_AGE_MS: u64 = 55 * 60 * 1000;

#[derive(Clone, Default, PartialEq, Eq)]
pub enum CodexTransport {
    #[default]
    Auto,
    Sse,
    WebSocket,
    WebSocketCached,
}

pub struct CodexTransportOptions {
    pub transport: CodexTransport,
    pub websocket_connect_timeout_ms: Option<u64>,
    pub session_id: Option<String>,
    pub signal: Option<CancellationToken>,
    pub env: Option<ProviderEnv>,
}

#[derive(Clone, Debug, Default)]
pub struct CodexWebSocketDebugStats {
    pub requests: u64,
    pub connections_created: u64,
    pub connections_reused: u64,
    pub cached_context_requests: u64,
    pub store_true_requests: u64,
    pub full_context_requests: u64,
    pub delta_requests: u64,
    pub last_input_items: u64,
    pub last_delta_input_items: Option<u64>,
    pub last_previous_response_id: Option<String>,
    pub websocket_failures: u64,
    pub sse_fallbacks: u64,
    pub websocket_fallback_active: Option<bool>,
    pub last_websocket_error: Option<String>,
}

#[derive(Clone)]
struct CachedWebSocketContinuationState {
    last_request_body: Value,
    last_response_id: String,
    last_response_items: Value,
}

struct CachedWebSocketConnection {
    socket: Arc<tokio::sync::Mutex<WsStream>>,
    busy: Arc<AtomicBool>,
    created_at: u64,
    continuation: Arc<Mutex<Option<CachedWebSocketContinuationState>>>,
    idle_task: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

struct WebSocketLease {
    socket: Arc<tokio::sync::Mutex<WsStream>>,
    entry: Option<Arc<CachedWebSocketConnection>>,
    session_id: Option<String>,
    reused: bool,
    ephemeral: bool,
}

static SSE_FALLBACK_SESSIONS: once_cell::sync::Lazy<Mutex<HashSet<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashSet::new()));
static WEBSOCKET_SESSION_CACHE: once_cell::sync::Lazy<Mutex<HashMap<String, Arc<CachedWebSocketConnection>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));
static WEBSOCKET_DEBUG_STATS: once_cell::sync::Lazy<Mutex<HashMap<String, CodexWebSocketDebugStats>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub fn compress_request_body_zstd(body_json: &str) -> Option<Vec<u8>> {
    zstd::encode_all(body_json.as_bytes(), 3).ok()
}

pub fn resolve_codex_websocket_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    let host = normalized.trim_start_matches("https://").trim_start_matches("http://");
    if normalized.ends_with("/codex/responses") {
        format!("wss://{host}")
    } else if normalized.ends_with("/codex") {
        format!("wss://{host}/responses")
    } else {
        format!("wss://{host}/codex/responses")
    }
}

pub fn is_websocket_sse_fallback_active(session_id: Option<&str>) -> bool {
    session_id
        .map(|id| SSE_FALLBACK_SESSIONS.lock().unwrap().contains(id))
        .unwrap_or(false)
}

fn mark_sse_fallback(session_id: Option<&str>) {
    if let Some(id) = session_id {
        SSE_FALLBACK_SESSIONS.lock().unwrap().insert(id.to_string());
        if let Ok(mut stats) = WEBSOCKET_DEBUG_STATS.lock() {
            let entry = stats.entry(id.to_string()).or_default();
            entry.sse_fallbacks += 1;
            entry.websocket_fallback_active = Some(true);
        }
    }
}

pub fn is_connection_limit_error(error: &str) -> bool {
    error.contains(WEBSOCKET_CONNECTION_LIMIT_REACHED)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn update_debug_stats(session_id: &str, update: impl FnOnce(&mut CodexWebSocketDebugStats)) {
    if let Ok(mut stats) = WEBSOCKET_DEBUG_STATS.lock() {
        let entry = stats.entry(session_id.to_string()).or_default();
        update(entry);
    }
}

pub fn get_codex_websocket_debug_stats(session_id: &str) -> Option<CodexWebSocketDebugStats> {
    WEBSOCKET_DEBUG_STATS.lock().ok()?.get(session_id).cloned()
}

pub fn reset_codex_websocket_debug_stats(session_id: Option<&str>) {
    if let Ok(mut stats) = WEBSOCKET_DEBUG_STATS.lock() {
        match session_id {
            Some(id) => {
                stats.remove(id);
                SSE_FALLBACK_SESSIONS.lock().unwrap().remove(id);
            }
            None => {
                stats.clear();
                SSE_FALLBACK_SESSIONS.lock().unwrap().clear();
            }
        }
    }
}

pub fn close_codex_websocket_sessions(session_id: Option<&str>) {
    let close_entry = |entry: &CachedWebSocketConnection| {
        if let Ok(mut task) = entry.idle_task.try_lock()
            && let Some(handle) = task.take()
        {
            handle.abort();
        }
        if let Ok(mut socket) = entry.socket.try_lock() {
            let _ = futures_util::future::FutureExt::now_or_never(socket.close(None));
        }
    };

    if let Ok(mut cache) = WEBSOCKET_SESSION_CACHE.lock() {
        match session_id {
            Some(id) => {
                if let Some(entry) = cache.remove(id) {
                    close_entry(&entry);
                }
                SSE_FALLBACK_SESSIONS.lock().unwrap().remove(id);
            }
            None => {
                for entry in cache.values() {
                    close_entry(entry);
                }
                cache.clear();
                SSE_FALLBACK_SESSIONS.lock().unwrap().clear();
            }
        }
    }
}

fn is_session_expired(entry: &CachedWebSocketConnection) -> bool {
    now_ms().saturating_sub(entry.created_at) >= SESSION_WEBSOCKET_MAX_AGE_MS
}

async fn close_socket_quietly(socket: &Arc<tokio::sync::Mutex<WsStream>>) {
    let mut guard = socket.lock().await;
    let _ = guard.close(None).await;
}

fn schedule_idle_expiry(session_id: String, entry: Arc<CachedWebSocketConnection>) {
    let socket = entry.socket.clone();
    let busy = entry.busy.clone();
    let task_slot = entry.idle_task.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(SESSION_WEBSOCKET_CACHE_TTL_MS)).await;
        if busy.load(Ordering::SeqCst) {
            return;
        }
        close_socket_quietly(&socket).await;
        if let Ok(mut cache) = WEBSOCKET_SESSION_CACHE.lock() {
            cache.remove(&session_id);
        }
    });
    if let Ok(mut slot) = task_slot.try_lock()
        && let Some(old) = slot.replace(handle)
    {
        old.abort();
    }
}

async fn connect_websocket(
    ws_url: &str,
    headers: &HashMap<String, String>,
    timeout_ms: u64,
    env: Option<&ProviderEnv>,
) -> Result<WsStream> {
    let mut request_headers = headers.clone();
    request_headers.insert("OpenAI-Beta".to_string(), OPENAI_BETA_RESPONSES_WEBSOCKETS.to_string());
    connect_websocket_with_proxy(ws_url, &request_headers, timeout_ms, env).await
}

async fn acquire_websocket(
    ws_url: &str,
    headers: &HashMap<String, String>,
    session_id: Option<&str>,
    timeout_ms: u64,
    env: Option<&ProviderEnv>,
) -> Result<WebSocketLease> {
    let Some(session_id) = session_id else {
        let socket = connect_websocket(ws_url, headers, timeout_ms, env).await?;
        return Ok(WebSocketLease {
            socket: Arc::new(tokio::sync::Mutex::new(socket)),
            entry: None,
            session_id: None,
            reused: false,
            ephemeral: true,
        });
    };

    enum CacheAction {
        Reuse(Arc<CachedWebSocketConnection>),
        Expired(Arc<CachedWebSocketConnection>),
        Busy,
    }

    let cache_action = WEBSOCKET_SESSION_CACHE.lock().ok().and_then(|mut cache| {
        let entry = cache.get(session_id)?.clone();
        if let Ok(mut task) = entry.idle_task.try_lock()
            && let Some(handle) = task.take()
        {
            handle.abort();
        }
        if entry.busy.load(Ordering::SeqCst) {
            return Some(CacheAction::Busy);
        }
        if is_session_expired(&entry) {
            cache.remove(session_id);
            return Some(CacheAction::Expired(entry));
        }
        entry.busy.store(true, Ordering::SeqCst);
        Some(CacheAction::Reuse(entry))
    });

    if let Some(action) = cache_action {
        match action {
            CacheAction::Reuse(entry) => {
                return Ok(WebSocketLease {
                    socket: entry.socket.clone(),
                    entry: Some(entry),
                    session_id: Some(session_id.to_string()),
                    reused: true,
                    ephemeral: false,
                });
            }
            CacheAction::Expired(entry) => {
                close_socket_quietly(&entry.socket).await;
            }
            CacheAction::Busy => {
                let socket = connect_websocket(ws_url, headers, timeout_ms, env).await?;
                return Ok(WebSocketLease {
                    socket: Arc::new(tokio::sync::Mutex::new(socket)),
                    entry: None,
                    session_id: Some(session_id.to_string()),
                    reused: false,
                    ephemeral: true,
                });
            }
        }
    }

    let socket = connect_websocket(ws_url, headers, timeout_ms, env).await?;
    let entry = Arc::new(CachedWebSocketConnection {
        socket: Arc::new(tokio::sync::Mutex::new(socket)),
        busy: Arc::new(AtomicBool::new(true)),
        created_at: now_ms(),
        continuation: Arc::new(Mutex::new(None)),
        idle_task: Arc::new(tokio::sync::Mutex::new(None)),
    });
    WEBSOCKET_SESSION_CACHE
        .lock()
        .unwrap()
        .insert(session_id.to_string(), entry.clone());

    Ok(WebSocketLease {
        socket: entry.socket.clone(),
        entry: Some(entry),
        session_id: Some(session_id.to_string()),
        reused: false,
        ephemeral: false,
    })
}

async fn release_websocket(lease: WebSocketLease, keep: bool) {
    if lease.ephemeral || lease.entry.is_none() {
        close_socket_quietly(&lease.socket).await;
        return;
    }

    let (Some(entry), Some(session_id)) = (lease.entry, lease.session_id) else {
        close_socket_quietly(&lease.socket).await;
        return;
    };

    if !keep {
        close_socket_quietly(&entry.socket).await;
        if let Ok(mut cache) = WEBSOCKET_SESSION_CACHE.lock() {
            cache.remove(&session_id);
        }
        return;
    }

    entry.busy.store(false, Ordering::SeqCst);
    schedule_idle_expiry(session_id, entry);
}

fn request_body_without_input(body: &Value) -> Value {
    let mut copy = body.clone();
    if let Some(obj) = copy.as_object_mut() {
        obj.remove("input");
        obj.remove("previous_response_id");
    }
    copy
}

fn response_inputs_equal(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

fn get_cached_websocket_input_delta(body: &Value, continuation: &CachedWebSocketContinuationState) -> Option<Value> {
    if !response_inputs_equal(
        &request_body_without_input(body),
        &request_body_without_input(&continuation.last_request_body),
    ) {
        return None;
    }

    let current_input = body.get("input").cloned().unwrap_or(Value::Array(vec![]));
    let baseline_items = continuation
        .last_request_body
        .get("input")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let response_items = continuation.last_response_items.clone();

    let baseline = match baseline_items {
        Value::Array(mut items) => {
            if let Value::Array(extra) = response_items {
                items.extend(extra);
            }
            items
        }
        _ => Vec::new(),
    };

    let current = match current_input {
        Value::Array(items) => items,
        _ => return None,
    };

    if current.len() < baseline.len() {
        return None;
    }

    let baseline_len = baseline.len();
    let prefix = Value::Array(current[..baseline_len].to_vec());
    let baseline_value = Value::Array(baseline);
    if !response_inputs_equal(&prefix, &baseline_value) {
        return None;
    }

    Some(Value::Array(current[baseline_len..].to_vec()))
}

fn build_cached_websocket_request_body(entry: &CachedWebSocketConnection, body: &Value) -> Value {
    let continuation = entry.continuation.lock().ok().and_then(|g| g.clone());
    let Some(continuation) = continuation else {
        return body.clone();
    };

    let delta = get_cached_websocket_input_delta(body, &continuation);
    let Some(delta) = delta else {
        if let Ok(mut guard) = entry.continuation.lock() {
            *guard = None;
        }
        return body.clone();
    };

    if continuation.last_response_id.is_empty() {
        if let Ok(mut guard) = entry.continuation.lock() {
            *guard = None;
        }
        return body.clone();
    }

    let mut cached = body.clone();
    if let Some(obj) = cached.as_object_mut() {
        obj.insert(
            "previous_response_id".to_string(),
            Value::String(continuation.last_response_id.clone()),
        );
        obj.insert("input".to_string(), delta);
    }
    cached
}

pub fn update_codex_websocket_continuation(
    session_id: &str,
    full_body: &Value,
    response_id: &str,
    response_items: Value,
) {
    let Ok(cache) = WEBSOCKET_SESSION_CACHE.lock() else {
        return;
    };
    let Some(entry) = cache.get(session_id) else {
        return;
    };
    if let Ok(mut guard) = entry.continuation.lock() {
        *guard = Some(CachedWebSocketContinuationState {
            last_request_body: full_body.clone(),
            last_response_id: response_id.to_string(),
            last_response_items: response_items,
        });
    }
}

pub fn clear_codex_websocket_continuation(session_id: &str) {
    if let Ok(cache) = WEBSOCKET_SESSION_CACHE.lock()
        && let Some(entry) = cache.get(session_id)
        && let Ok(mut guard) = entry.continuation.lock()
    {
        *guard = None;
    }
}

pub struct CodexCollectResult {
    pub events: Vec<Value>,
    pub websocket_reused: bool,
    pub used_cached_context: bool,
}

pub async fn collect_codex_events(
    base_url: &str,
    body: Value,
    headers: HashMap<String, String>,
    client: &reqwest::Client,
    sse_url: &str,
    options: &CodexTransportOptions,
) -> Result<Vec<Value>> {
    collect_codex_events_detailed(base_url, body, headers, client, sse_url, options)
        .await
        .map(|result| result.events)
}

pub async fn collect_codex_events_detailed(
    base_url: &str,
    body: Value,
    headers: HashMap<String, String>,
    client: &reqwest::Client,
    sse_url: &str,
    options: &CodexTransportOptions,
) -> Result<CodexCollectResult> {
    let transport = options.transport.clone();
    let session_id = options.session_id.as_deref();
    let timeout_ms = options.websocket_connect_timeout_ms.unwrap_or(30_000);

    if transport != CodexTransport::Sse && !is_websocket_sse_fallback_active(session_id) {
        let use_cached_context = transport == CodexTransport::WebSocketCached || transport == CodexTransport::Auto;
        match try_websocket_stream(base_url, &body, &headers, options, use_cached_context, timeout_ms).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                let msg = error.to_string();
                if is_connection_limit_error(&msg) && transport == CodexTransport::Auto {
                    // retry once without session cache entry
                } else {
                    mark_sse_fallback(session_id);
                    if let Some(id) = session_id {
                        update_debug_stats(id, |stats| {
                            stats.websocket_failures += 1;
                            stats.last_websocket_error = Some(msg.clone());
                            stats.websocket_fallback_active = Some(true);
                        });
                    }
                }
            }
        }
    }

    let events = collect_codex_sse_events(client, sse_url, &body, &headers, &options.signal).await?;
    Ok(CodexCollectResult {
        events,
        websocket_reused: false,
        used_cached_context: false,
    })
}

async fn try_websocket_stream(
    base_url: &str,
    body: &Value,
    headers: &HashMap<String, String>,
    options: &CodexTransportOptions,
    use_cached_context: bool,
    timeout_ms: u64,
) -> Result<CodexCollectResult> {
    let ws_url = resolve_codex_websocket_url(base_url);
    let lease = acquire_websocket(
        &ws_url,
        headers,
        options.session_id.as_deref(),
        timeout_ms,
        options.env.as_ref(),
    )
    .await?;

    if let (Some(session_id), true) = (options.session_id.as_deref(), lease.reused) {
        update_debug_stats(session_id, |stats| {
            stats.connections_reused += 1;
        });
    } else if let Some(session_id) = options.session_id.as_deref() {
        update_debug_stats(session_id, |stats| {
            stats.connections_created += 1;
        });
    }

    let request_body = if use_cached_context {
        if let Some(entry) = &lease.entry {
            build_cached_websocket_request_body(entry, body)
        } else {
            body.clone()
        }
    } else {
        body.clone()
    };

    if let Some(session_id) = options.session_id.as_deref() {
        update_debug_stats(session_id, |stats| {
            stats.requests += 1;
            if use_cached_context {
                stats.cached_context_requests += 1;
            }
            if request_body.get("store").and_then(|v| v.as_bool()) == Some(true) {
                stats.store_true_requests += 1;
            }
            stats.last_input_items = request_body
                .get("input")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as u64)
                .unwrap_or(0);
            if let Some(prev) = request_body.get("previous_response_id").and_then(|v| v.as_str()) {
                stats.delta_requests += 1;
                stats.last_delta_input_items = request_body
                    .get("input")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len() as u64);
                stats.last_previous_response_id = Some(prev.to_string());
            } else {
                stats.full_context_requests += 1;
                stats.last_delta_input_items = None;
                stats.last_previous_response_id = None;
            }
        });
    }

    let reused = lease.reused;
    let collect_result = collect_websocket_events(&lease.socket, &request_body).await;
    let keep = collect_result.is_ok();
    if !keep
        && let Some(entry) = &lease.entry
        && let Ok(mut guard) = entry.continuation.lock()
    {
        *guard = None;
    }
    release_websocket(lease, keep).await;
    let events = collect_result?;
    Ok(CodexCollectResult {
        events,
        websocket_reused: reused,
        used_cached_context: use_cached_context,
    })
}

async fn collect_websocket_events(socket: &Arc<tokio::sync::Mutex<WsStream>>, body: &Value) -> Result<Vec<Value>> {
    let mut payload = body.clone();
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("type".to_string(), Value::String("response.create".to_string()));
    } else {
        payload = serde_json::json!({ "type": "response.create", "body": body });
    }

    {
        let mut guard = socket.lock().await;
        guard.send(WsMessage::Text(payload.to_string().into())).await?;
    }

    let mut events = Vec::new();
    loop {
        let msg = {
            let mut guard = socket.lock().await;
            guard.next().await
        };
        let Some(msg) = msg else {
            break;
        };
        let msg = msg?;
        if let WsMessage::Text(text) = msg {
            let event: Value = serde_json::from_str(&text)?;
            if event.get("type").and_then(|v| v.as_str()) == Some("error") {
                let code = event.pointer("/error/code").and_then(|v| v.as_str()).unwrap_or("");
                if code == WEBSOCKET_CONNECTION_LIMIT_REACHED {
                    return Err(anyhow!(WEBSOCKET_CONNECTION_LIMIT_REACHED));
                }
                let message = event
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Codex WebSocket error");
                return Err(anyhow!("{message}"));
            }
            let event_type = event.get("type").and_then(|v| v.as_str()).map(|s| s.to_string());
            events.push(event);
            if matches!(
                event_type.as_deref(),
                Some("response.done") | Some("response.completed") | Some("response.incomplete")
            ) {
                break;
            }
        }
    }
    Ok(events)
}

/// Exposed for integration tests (pi-ai codex websocket cache parity).
#[doc(hidden)]
pub fn get_codex_websocket_input_delta(
    body: &Value,
    last_request_body: &Value,
    last_response_items: &Value,
) -> Option<Value> {
    let continuation = CachedWebSocketContinuationState {
        last_request_body: last_request_body.clone(),
        last_response_id: "resp_test".to_string(),
        last_response_items: last_response_items.clone(),
    };
    get_cached_websocket_input_delta(body, &continuation)
}

async fn collect_codex_sse_events(
    client: &reqwest::Client,
    url: &str,
    body: &Value,
    headers: &HashMap<String, String>,
    signal: &Option<CancellationToken>,
) -> Result<Vec<Value>> {
    let body_json = serde_json::to_string(body)?;
    let mut req = if let Some(compressed) = compress_request_body_zstd(&body_json) {
        let mut r = client.post(url).body(compressed);
        r = r.header("Content-Type", "application/json");
        r = r.header("Content-Encoding", "zstd");
        r
    } else {
        client.post(url).json(body)
    };
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let response = send_with_abort(signal, req).await?;
    let response = crate::api::common::check_response_ok(response).await?;
    let mut events = Vec::new();
    for_each_sse_json_event(response, signal, |event| {
        events.push(event);
        Ok(())
    })
    .await?;
    Ok(events)
}
