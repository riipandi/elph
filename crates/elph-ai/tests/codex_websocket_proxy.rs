use std::collections::HashMap;

use elph_ai::api::websocket_connect::connect_websocket_with_proxy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn mock_http_proxy(target_host: &str, target_port: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let expected = format!("CONNECT {target_host}:{target_port}");

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = vec![0u8; 4096];
            let read = socket.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]);
            if request.contains(&expected) {
                let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
            }
        }
    });

    format!("http://127.0.0.1:{port}", port = addr.port())
}

#[tokio::test]
async fn sends_connect_request_through_http_proxy() {
    let proxy_url = mock_http_proxy("chatgpt.com", 443).await;
    let mut env = HashMap::new();
    env.insert("HTTPS_PROXY".to_string(), proxy_url);

    let result = connect_websocket_with_proxy(
        "wss://chatgpt.com/backend-api/codex/responses",
        &HashMap::new(),
        2_000,
        Some(&env),
    )
    .await;

    let err = result
        .err()
        .expect("expected TLS handshake to fail against mock proxy tunnel");
    let message = err.to_string();
    assert!(
        message.contains("TLS handshake failed") || message.contains("WebSocket connect timeout"),
        "unexpected error: {message}"
    );
}
