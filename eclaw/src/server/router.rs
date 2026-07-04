use axum::{Router, middleware};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::server::{error, middleware::trim_trailing_slash, routes};

pub fn build() -> Router {
    routes::router()
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .layer(middleware::from_fn(trim_trailing_slash))
        .layer(CatchPanicLayer::custom(|_| error::panic_response()))
        .layer(TraceLayer::new_for_http())
}
