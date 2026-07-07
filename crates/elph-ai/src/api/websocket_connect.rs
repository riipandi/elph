//! WebSocket TCP/TLS connect with optional HTTP(S) proxy tunnel (mirroring pi-ai Codex).

use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use anyhow::{Context as AnyhowContext, Result, anyhow, bail};
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use url::Url;

use crate::api::http_proxy::{resolve_http_proxy_url_for_target, websocket_proxy_lookup_url};
use crate::types::ProviderEnv;

pub type WsStream = WebSocketStream<CodexWsIo>;

/// IO layer for Codex WebSockets (direct TLS, plain, or nested TLS through HTTPS proxy).
pub enum CodexWsIo {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    TlsOverProxyTls(Box<TlsStream<TlsStream<TcpStream>>>),
}

impl AsyncRead for CodexWsIo {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            CodexWsIo::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            CodexWsIo::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
            CodexWsIo::TlsOverProxyTls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for CodexWsIo {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            CodexWsIo::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            CodexWsIo::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
            CodexWsIo::TlsOverProxyTls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            CodexWsIo::Plain(stream) => Pin::new(stream).poll_flush(cx),
            CodexWsIo::Tls(stream) => Pin::new(stream).poll_flush(cx),
            CodexWsIo::TlsOverProxyTls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            CodexWsIo::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            CodexWsIo::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
            CodexWsIo::TlsOverProxyTls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

fn ensure_crypto_provider() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn shared_tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            ensure_crypto_provider();
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

fn parse_websocket_endpoint(ws_url: &str) -> Result<(bool, String, u16)> {
    let url = Url::parse(ws_url).with_context(|| format!("invalid WebSocket URL: {ws_url}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("WebSocket URL missing host: {ws_url}"))?
        .to_string();
    let secure = matches!(url.scheme(), "wss" | "https");
    let port = url.port().unwrap_or(if secure { 443 } else { 80 });
    Ok((secure, host, port))
}

fn proxy_endpoint(proxy_url: &Url) -> Result<(String, u16)> {
    let host = proxy_url
        .host_str()
        .ok_or_else(|| anyhow!("proxy URL missing host"))?
        .to_string();
    let port = proxy_url
        .port()
        .unwrap_or(if proxy_url.scheme() == "https" { 443 } else { 80 });
    Ok((host, port))
}

async fn read_until_headers_end(stream: &mut (impl AsyncReadExt + Unpin)) -> Result<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            bail!("proxy closed before CONNECT response");
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 16_384 {
            bail!("proxy CONNECT response too large");
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn ensure_connect_success(response: &str) -> Result<()> {
    let status_line = response.lines().next().unwrap_or_default();
    if status_line.contains(" 200 ") {
        return Ok(());
    }
    bail!("proxy CONNECT failed: {status_line}")
}

async fn send_http_connect(
    stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin),
    target_host: &str,
    target_port: u16,
) -> Result<()> {
    let request = format!("CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let response = read_until_headers_end(stream).await?;
    ensure_connect_success(&response)
}

async fn tls_handshake<S>(server_name: &str, stream: S) -> Result<TlsStream<S>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let dns_name = ServerName::try_from(server_name.to_string()).map_err(|_| anyhow!("invalid TLS server name"))?;
    let connector = TlsConnector::from(shared_tls_config());
    connector
        .connect(dns_name, stream)
        .await
        .map_err(|error| anyhow!("TLS handshake failed for {server_name}: {error}"))
}

async fn open_stream(ws_url: &str, env: Option<&ProviderEnv>) -> Result<CodexWsIo> {
    let (secure, host, port) = parse_websocket_endpoint(ws_url)?;
    let lookup_url = websocket_proxy_lookup_url(ws_url);
    let proxy = resolve_http_proxy_url_for_target(&lookup_url, env)?;

    let Some(proxy_url) = proxy else {
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .with_context(|| format!("failed to connect to {host}:{port}"))?;
        return if secure {
            Ok(CodexWsIo::Tls(Box::new(tls_handshake(&host, tcp).await?)))
        } else {
            Ok(CodexWsIo::Plain(tcp))
        };
    };

    let (proxy_host, proxy_port) = proxy_endpoint(&proxy_url)?;
    let tcp = TcpStream::connect((proxy_host.as_str(), proxy_port))
        .await
        .with_context(|| format!("failed to connect to proxy {proxy_host}:{proxy_port}"))?;

    if proxy_url.scheme() == "https" {
        let mut proxy_tls = tls_handshake(proxy_host.as_str(), tcp).await?;
        send_http_connect(&mut proxy_tls, &host, port).await?;
        if secure {
            let target_tls = tls_handshake(&host, proxy_tls).await?;
            Ok(CodexWsIo::TlsOverProxyTls(Box::new(target_tls)))
        } else {
            bail!("HTTPS proxy with non-secure WebSocket target is unsupported");
        }
    } else {
        let mut stream = tcp;
        send_http_connect(&mut stream, &host, port).await?;
        if secure {
            Ok(CodexWsIo::Tls(Box::new(tls_handshake(&host, stream).await?)))
        } else {
            Ok(CodexWsIo::Plain(stream))
        }
    }
}

/// Open a WebSocket connection, optionally routing through HTTP proxy env vars.
pub async fn connect_websocket_with_proxy(
    ws_url: &str,
    headers: &std::collections::HashMap<String, String>,
    timeout_ms: u64,
    env: Option<&ProviderEnv>,
) -> Result<WsStream> {
    let mut request = ws_url.into_client_request()?;
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("accept") {
            continue;
        }
        request.headers_mut().insert(
            http::HeaderName::from_bytes(k.as_bytes()).map_err(|e| anyhow!("invalid header name {k}: {e}"))?,
            HeaderValue::from_str(v).map_err(|e| anyhow!("invalid header {k}: {e}"))?,
        );
    }

    let timeout = std::time::Duration::from_millis(timeout_ms);
    let connect = tokio::time::timeout(timeout, async {
        let stream = open_stream(ws_url, env).await?;
        client_async(request, stream)
            .await
            .map(|(socket, _)| socket)
            .map_err(Into::into)
    });
    connect.await.map_err(|_| anyhow!("WebSocket connect timeout"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wss_endpoint() {
        let (secure, host, port) = parse_websocket_endpoint("wss://chatgpt.com/backend-api/codex/responses").unwrap();
        assert!(secure);
        assert_eq!(host, "chatgpt.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn parses_ws_custom_port() {
        let (secure, host, port) = parse_websocket_endpoint("ws://localhost:9001/ws").unwrap();
        assert!(!secure);
        assert_eq!(host, "localhost");
        assert_eq!(port, 9001);
    }

    #[test]
    fn connect_response_must_be_200() {
        ensure_connect_success("HTTP/1.1 200 Connection Established\r\n\r\n").unwrap();
        assert!(ensure_connect_success("HTTP/1.1 403 Forbidden\r\n\r\n").is_err());
    }
}
