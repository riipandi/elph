mod api;
mod root;
mod rpc;
#[cfg(feature = "web")]
mod web;

use axum::Router;
use axum::routing::get;

pub fn router() -> Router {
    let router = Router::new().route("/", get(root::root)).nest("/api", api::router());

    #[cfg(feature = "web")]
    let router = router.nest("/ui", web::router());

    router.nest("/rpc", rpc::router())
}
