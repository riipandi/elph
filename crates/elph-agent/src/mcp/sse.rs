//! Legacy MCP HTTP+SSE client transport (pre–Streamable HTTP).
//!
//! Protocol (2024-11-05 style):
//! 1. Client opens `GET` on the SSE URL with `Accept: text/event-stream`.
//! 2. Server sends an `endpoint` event whose data is the POST message URL.
//! 3. Client POSTs JSON-RPC messages to that endpoint.
//! 4. Responses and notifications arrive as SSE `message` events.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use http::{HeaderName, HeaderValue};
use rmcp::RoleClient;
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};

use super::config::McpHttpConfig;

const ENDPOINT_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum SseTransportError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("SSE stream ended before endpoint event")]
    NoEndpoint,
    #[error("endpoint wait timed out")]
    EndpointTimeout,
    #[error("channel error: {0}")]
    Channel(String),
}

/// Legacy SSE MCP transport implementing [`Transport`] for [`RoleClient`].
pub struct SseClientTransport {
    outbound: mpsc::UnboundedSender<ClientJsonRpcMessage>,
    inbound: mpsc::UnboundedReceiver<ServerJsonRpcMessage>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl SseClientTransport {
    /// Connect to an SSE MCP endpoint described by `config` (static bearer only).
    #[allow(dead_code)]
    pub async fn connect(config: &McpHttpConfig) -> Result<Self, SseTransportError> {
        Self::connect_with_bearer(config, None).await
    }

    /// Connect with an optional bearer override (e.g. OAuth access token).
    ///
    /// Priority: `bearer_override` → `config.resolve_auth_token()`.
    pub async fn connect_with_bearer(
        config: &McpHttpConfig,
        bearer_override: Option<String>,
    ) -> Result<Self, SseTransportError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|e| SseTransportError::Http(e.to_string()))?;

        let auth_token = bearer_override.or_else(|| config.resolve_auth_token());

        let mut request = client
            .get(&config.url)
            .header(http::header::ACCEPT, "text/event-stream")
            .header(http::header::CACHE_CONTROL, "no-cache");

        if let Some(token) = &auth_token {
            request = request.bearer_auth(token);
        }
        for (key, value) in &config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        request = crate::trace::with_trace_headers(request);

        let response = request
            .send()
            .await
            .map_err(|e| SseTransportError::Http(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SseTransportError::Http(format!(
                "SSE GET {} returned {}",
                config.url,
                response.status()
            )));
        }

        let base_url = config.url.clone();
        let headers = config.headers.clone();

        let (endpoint_tx, endpoint_rx) = oneshot::channel::<String>();
        let endpoint_tx = Arc::new(Mutex::new(Some(endpoint_tx)));

        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<ClientJsonRpcMessage>();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // SSE reader task
        let endpoint_for_reader = Arc::clone(&endpoint_tx);
        let inbound_for_reader = inbound_tx.clone();
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut event_name = String::new();
            let mut data_lines: Vec<String> = Vec::new();

            while let Some(chunk) = stream.next().await {
                let Ok(bytes) = chunk else {
                    break;
                };
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(idx) = buffer.find('\n') {
                    let mut line = buffer[..idx].to_string();
                    buffer.drain(..=idx);
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    if line.is_empty() {
                        // Dispatch event
                        let data = data_lines.join("\n");
                        let name = if event_name.is_empty() {
                            "message".to_string()
                        } else {
                            event_name.clone()
                        };
                        event_name.clear();
                        data_lines.clear();
                        if data.is_empty() {
                            continue;
                        }
                        if name == "endpoint" {
                            let mut guard = endpoint_for_reader.lock().await;
                            if let Some(tx) = guard.take() {
                                let _ = tx.send(data);
                            }
                        } else {
                            // message / default — JSON-RPC server message
                            match serde_json::from_str::<ServerJsonRpcMessage>(&data) {
                                Ok(msg) => {
                                    if inbound_for_reader.send(msg).is_err() {
                                        return;
                                    }
                                }
                                Err(error) => {
                                    // Some servers wrap payload
                                    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&data)
                                        && let Some(inner) = map.get("message")
                                        && let Ok(msg) = serde_json::from_value::<ServerJsonRpcMessage>(inner.clone())
                                    {
                                        let _ = inbound_for_reader.send(msg);
                                    } else {
                                        log::debug!("SSE skip non-JSON-RPC event: {error}");
                                    }
                                }
                            }
                        }
                    } else if let Some(rest) = line.strip_prefix("event:") {
                        event_name = rest.trim().to_string();
                    } else if let Some(rest) = line.strip_prefix("data:") {
                        data_lines.push(rest.trim_start().to_string());
                    } else if line.starts_with(':') {
                        // comment
                    }
                }
            }
            log::debug!("SSE stream closed");
        });

        // Wait for endpoint
        let endpoint_path = tokio::time::timeout(ENDPOINT_WAIT, endpoint_rx)
            .await
            .map_err(|_| SseTransportError::EndpointTimeout)?
            .map_err(|_| SseTransportError::NoEndpoint)?;

        let message_url = resolve_endpoint_url(&base_url, &endpoint_path).map_err(SseTransportError::Http)?;
        log::debug!("SSE message endpoint ready: {message_url}");

        // POST sender task
        let post_client = client;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    msg = outbound_rx.recv() => {
                        let Some(msg) = msg else { break; };
                        let mut req = post_client
                            .post(&message_url)
                            .header(http::header::CONTENT_TYPE, "application/json")
                            .header(http::header::ACCEPT, "application/json, text/event-stream");
                        if let Some(token) = &auth_token {
                            req = req.bearer_auth(token);
                        }
                        for (k, v) in &headers {
                            req = req.header(k.as_str(), v.as_str());
                        }
                        req = crate::trace::with_trace_headers(req);
                        match req.json(&msg).send().await {
                            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 202 => {
                                // 200 may include JSON body as immediate response (some servers).
                                if let Ok(bytes) = resp.bytes().await
                                    && !bytes.is_empty()
                                    && let Ok(server_msg) = serde_json::from_slice::<ServerJsonRpcMessage>(&bytes)
                                {
                                    let _ = inbound_tx.send(server_msg);
                                }
                            }
                            Ok(resp) => {
                                log::warn!("SSE POST message failed: status={}", resp.status());
                            }
                            Err(error) => {
                                log::warn!("SSE POST message error: {error}");
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            outbound: outbound_tx,
            inbound: inbound_rx,
            shutdown: Some(shutdown_tx),
        })
    }
}

impl Transport<RoleClient> for SseClientTransport {
    type Error = SseTransportError;

    fn name() -> Cow<'static, str> {
        Cow::Borrowed("mcp-sse-client")
    }

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        let outbound = self.outbound.clone();
        async move {
            outbound
                .send(item)
                .map_err(|e| SseTransportError::Channel(e.to_string()))
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.inbound.recv().await
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

fn resolve_endpoint_url(base: &str, endpoint: &str) -> Result<String, String> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    let base = url::Url::parse(base).map_err(|e| e.to_string())?;
    let joined = base.join(endpoint).map_err(|e| e.to_string())?;
    Ok(joined.to_string())
}

/// Build header map for SSE (validation only; used by callers if needed).
#[allow(dead_code)]
pub fn parse_headers(headers: &HashMap<String, String>) -> Result<HashMap<HeaderName, HeaderValue>, String> {
    let mut out = HashMap::new();
    for (k, v) in headers {
        let name = HeaderName::from_bytes(k.as_bytes()).map_err(|e| e.to_string())?;
        let value = HeaderValue::from_str(v).map_err(|e| e.to_string())?;
        out.insert(name, value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_endpoint() {
        let url = resolve_endpoint_url("http://localhost:3000/sse", "/messages?session=1").unwrap();
        assert_eq!(url, "http://localhost:3000/messages?session=1");
    }

    #[test]
    fn resolve_absolute_endpoint() {
        let url = resolve_endpoint_url("http://localhost:3000/sse", "http://localhost:3000/msg").unwrap();
        assert_eq!(url, "http://localhost:3000/msg");
    }
}
