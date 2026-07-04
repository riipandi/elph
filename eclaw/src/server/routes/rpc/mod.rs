use axum::{Json, Router, http::StatusCode, routing::get};
use serde::Serialize;

#[derive(Serialize)]
struct PlaceholderResponse {
    message: &'static str,
}

pub fn router() -> Router {
    Router::new().route("/", get(index))
}

async fn index() -> (StatusCode, Json<PlaceholderResponse>) {
    (
        StatusCode::OK,
        Json(PlaceholderResponse {
            message: "rpc — not yet implemented",
        }),
    )
}
