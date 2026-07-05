use axum::{Router, middleware};
use tower_http::catch_panic::CatchPanicLayer;

use crate::server::{
    error,
    middleware::{request_logger, trim_trailing_slash},
    routes,
};

pub fn build() -> Router {
    routes::router()
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .layer(middleware::from_fn(trim_trailing_slash))
        .layer(middleware::from_fn(request_logger))
        .layer(CatchPanicLayer::custom(|_| error::panic_response()))
}
