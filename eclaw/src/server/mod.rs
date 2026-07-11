mod config;
mod error;
mod middleware;
mod router;
mod routes;
#[cfg(feature = "web")]
mod web_assets;

pub use config::ServerConfig;
#[allow(unused_imports)]
pub use error::AppError;
pub use router::build;

use anyhow::Result;
use axum::serve;
use elph_agent::try_block_on;
use tokio::net::TcpListener;
use tokio::signal;

pub async fn run(config: ServerConfig) -> Result<()> {
    let addr = config.socket_addr()?;
    let app = build();

    tracing::info!(%addr, "eclaw server listening");

    let listener = TcpListener::bind(addr).await?;
    serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("eclaw server shutting down");
}

pub fn run_blocking(config: ServerConfig) -> Result<()> {
    try_block_on(run(config))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    async fn body_string(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[tokio::test]
    async fn root_returns_running_message() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_string(response).await;
        assert!(body.contains("eclaw is running"));
    }

    #[tokio::test]
    async fn trailing_slash_redirects_to_trimmed_path() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/api/health/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/api/health");
    }

    #[tokio::test]
    async fn health_route_returns_ok() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rpc_route_returns_placeholder() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/rpc").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_string(response).await;
        assert!(body.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn unknown_route_returns_json_error() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/api/missing").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
        let body = body_string(response).await;
        assert!(body.contains("\"status\":404"));
        assert!(body.contains("not found"));
    }

    #[tokio::test]
    async fn method_not_allowed_returns_json_error() {
        let app = build();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
        let body = body_string(response).await;
        assert!(body.contains("\"status\":405"));
    }

    #[cfg(not(feature = "web"))]
    #[tokio::test]
    async fn ui_route_unavailable_without_web() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/ui").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_string(response).await;
        assert!(body.contains("\"status\":404"));
    }

    #[cfg(feature = "web")]
    #[tokio::test]
    async fn ui_root_serves_index_html() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/ui").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_string(response).await;
        assert!(body.contains("eclaw UI"));
    }

    #[cfg(feature = "web")]
    #[tokio::test]
    async fn ui_trailing_slash_redirects_to_trimmed_path() {
        let app = build();
        let response = app
            .oneshot(Request::builder().uri("/ui/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/ui");
    }

    #[cfg(feature = "web")]
    #[tokio::test]
    async fn ui_unknown_route_falls_back_to_index_html() {
        let app = build();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ui/settings/profile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body_string(response).await;
        assert!(body.contains("eclaw UI"));
    }
}
