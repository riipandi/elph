use axum::{
    Json,
    body::Body,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub status: u16,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

impl ErrorResponse {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                status: status.as_u16(),
                message: message.into(),
            },
        }
    }

    pub fn into_response(self, status: StatusCode) -> Response {
        (status, [("content-type", "application/json")], Json(self)).into_response()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    #[allow(dead_code)]
    Internal(String),
}

// AppError variants/helpers are reserved for handler Result<T, AppError> wiring;
// the type is currently unused so rustc treats private methods as dead even when
// referenced from IntoResponse.
#[allow(dead_code, reason = "reserved AppError API until handlers return AppError")]
impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::BadRequest(message) | Self::NotFound(message) | Self::Internal(message) => message,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        ErrorResponse::new(status, self.message()).into_response(status)
    }
}

pub async fn not_found(req: Request<Body>) -> Response {
    ErrorResponse::new(StatusCode::NOT_FOUND, format!("route `{}` not found", req.uri().path()))
        .into_response(StatusCode::NOT_FOUND)
}

pub async fn method_not_allowed(req: Request<Body>) -> Response {
    ErrorResponse::new(
        StatusCode::METHOD_NOT_ALLOWED,
        format!("method `{}` not allowed for `{}`", req.method(), req.uri().path()),
    )
    .into_response(StatusCode::METHOD_NOT_ALLOWED)
}

#[allow(dead_code)]
pub async fn json_rejection(err: JsonRejection) -> Response {
    let (status, message) = match err {
        JsonRejection::JsonDataError(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
        JsonRejection::JsonSyntaxError(error) => (StatusCode::BAD_REQUEST, error.to_string()),
        JsonRejection::MissingJsonContentType(_) => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected application/json content-type".to_string(),
        ),
        _ => (StatusCode::BAD_REQUEST, "invalid json request".to_string()),
    };

    ErrorResponse::new(status, message).into_response(status)
}

#[allow(dead_code)]
pub async fn path_rejection(err: PathRejection) -> Response {
    ErrorResponse::new(StatusCode::BAD_REQUEST, err.to_string()).into_response(StatusCode::BAD_REQUEST)
}

#[allow(dead_code)]
pub async fn query_rejection(err: QueryRejection) -> Response {
    ErrorResponse::new(StatusCode::BAD_REQUEST, err.to_string()).into_response(StatusCode::BAD_REQUEST)
}

pub fn panic_response() -> Response {
    ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        .into_response(StatusCode::INTERNAL_SERVER_ERROR)
}
