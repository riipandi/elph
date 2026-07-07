use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use std::sync::Mutex;

use tokio::sync::oneshot;

use super::pages::{oauth_error_html, oauth_success_html};

#[derive(Debug, Clone)]
pub struct CallbackResult {
    pub code: String,
    pub state: Option<String>,
}

pub struct CallbackServer {
    shutdown: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    result_rx: oneshot::Receiver<Option<CallbackResult>>,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn start_callback_server(
    port: u16,
    path: &str,
    expected_state: Option<&str>,
    success_title: &str,
) -> anyhow::Result<CallbackServer> {
    let host = std::env::var("PI_OAUTH_CALLBACK_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let path = path.to_string();
    let expected_state = expected_state.map(|s| s.to_string());
    let success_title = success_title.to_string();

    let (result_tx, result_rx) = oneshot::channel();
    let result_tx = Arc::new(tokio::sync::Mutex::new(Some(result_tx)));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown = Arc::new(Mutex::new(Some(shutdown_tx)));

    let app = Router::new().route(
        &path,
        get({
            let result_tx = result_tx.clone();
            let expected_state = expected_state.clone();
            let success_title = success_title.clone();
            move |Query(query): Query<CallbackQuery>| {
                let result_tx = result_tx.clone();
                let expected_state = expected_state.clone();
                let success_title = success_title.clone();
                async move {
                    if let Some(error) = query.error {
                        return Html(oauth_error_html("Authentication did not complete.", Some(&error)));
                    }
                    let Some(code) = query.code else {
                        return Html(oauth_error_html("Missing authorization code.", None));
                    };
                    if let Some(ref expected) = expected_state
                        && query.state.as_deref() != Some(expected.as_str())
                    {
                        return Html(oauth_error_html("State mismatch.", None));
                    }
                    if let Some(tx) = result_tx.lock().await.take() {
                        let _ = tx.send(Some(CallbackResult {
                            code,
                            state: query.state,
                        }));
                    }
                    Html(oauth_success_html(&success_title))
                }
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });

    tokio::spawn(async move {
        let _ = server.await;
    });

    Ok(CallbackServer { shutdown, result_rx })
}

impl CallbackServer {
    #[allow(dead_code)]
    pub fn cancel_wait(&self) {
        if let Some(tx) = self.shutdown.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
    }

    pub async fn wait_for_code(self, timeout: std::time::Duration) -> Option<CallbackResult> {
        let shutdown = self.shutdown.clone();
        tokio::select! {
            result = self.result_rx => result.ok().flatten(),
            _ = tokio::time::sleep(timeout) => {
                if let Some(tx) = shutdown.lock().ok().and_then(|mut g| g.take()) {
                    let _ = tx.send(());
                }
                None
            }
        }
    }

    #[allow(dead_code)]
    pub fn close(self) {
        if let Some(tx) = self.shutdown.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
    }
}

pub fn parse_authorization_input(input: &str) -> (Option<String>, Option<String>) {
    let value = input.trim();
    if value.is_empty() {
        return (None, None);
    }
    if let Ok(url) = url::Url::parse(value) {
        return (
            url.query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.into_owned()),
            url.query_pairs()
                .find(|(k, _)| k == "state")
                .map(|(_, v)| v.into_owned()),
        );
    }
    if let Some((code, state)) = value.split_once('#') {
        return (Some(code.to_string()), Some(state.to_string()));
    }
    if value.contains("code=") {
        let params: Vec<_> = url::form_urlencoded::parse(value.as_bytes()).collect();
        let code = params.iter().find(|(k, _)| k == "code").map(|(_, v)| v.to_string());
        let state = params.iter().find(|(k, _)| k == "state").map(|(_, v)| v.to_string());
        return (code, state);
    }
    (Some(value.to_string()), None)
}
