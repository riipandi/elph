#![cfg(feature = "web")]

use crate::server::{error::ErrorResponse, web_assets::WebAssets};
use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
    response::Response,
    routing::get,
};

pub fn router() -> Router {
    Router::new()
        .route("/", get(|| async { serve("index.html").await }))
        .route("/{*path}", get(serve_path))
}

async fn serve_path(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    if let Some(response) = try_serve(&path) {
        return response;
    }

    serve("index.html").await
}

async fn serve(path: &str) -> Response {
    try_serve(path).unwrap_or_else(|| {
        ErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("embedded web asset `{path}` not found"),
        )
        .into_response(StatusCode::INTERNAL_SERVER_ERROR)
    })
}

fn try_serve(path: &str) -> Option<Response> {
    let file = WebAssets::get(path)?;
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(path))
            .body(Body::from(file.data))
            .expect("valid embedded response"),
    )
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("js") | Some("mjs") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
