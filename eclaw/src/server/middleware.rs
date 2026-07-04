use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

pub async fn trim_trailing_slash(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path();
    if path.len() > 1 && path.ends_with('/') {
        let trimmed = &path[..path.len() - 1];
        let location = match request.uri().query() {
            Some(query) => format!("{trimmed}?{query}"),
            None => trimmed.to_string(),
        };
        return Redirect::permanent(&location).into_response();
    }

    next.run(request).await
}
