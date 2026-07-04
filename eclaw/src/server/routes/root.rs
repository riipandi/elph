use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct RootResponse {
    pub message: &'static str,
}

pub async fn root() -> (StatusCode, Json<RootResponse>) {
    (
        StatusCode::OK,
        Json(RootResponse {
            message: "eclaw is running",
        }),
    )
}
