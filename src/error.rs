use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Application error type that converts to HTTP responses.
#[derive(Debug)]
pub enum AppError {
    /// 401 Unauthorized
    Unauthorized(String),
    /// 400 Bad Request
    BadRequest(String),
    /// 404 Not Found
    NotFound(String),
    /// 500 Internal Server Error
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

/// Extension type for request-scoped user identity.
#[derive(Debug, Clone)]
pub struct UserIdExtension(pub String);

mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_error_display() {
        let err = AppError::Unauthorized("missing token".into());
        assert_eq!(format!("{}", err), "Unauthorized: missing token");

        let err = AppError::BadRequest("invalid body".into());
        assert_eq!(format!("{}", err), "Bad Request: invalid body");

        let err = AppError::NotFound("no account".into());
        assert_eq!(format!("{}", err), "Not Found: no account");

        let err = AppError::Internal("db down".into());
        assert_eq!(format!("{}", err), "Internal Error: db down");
    }

    #[test]
    fn test_user_id_extension() {
        let ext = UserIdExtension("alice".into());
        assert_eq!(ext.0, "alice");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();
        match app_err {
            AppError::Internal(msg) => assert!(msg.contains("file not found")),
            _ => panic!("Expected Internal error"),
        }
    }

    #[tokio::test]
    async fn test_unauthorized_into_response() {
        let err = AppError::Unauthorized("bad token".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_bad_request_into_response() {
        let err = AppError::BadRequest("bad input".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_not_found_into_response() {
        let err = AppError::NotFound("missing".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_internal_into_response() {
        let err = AppError::Internal("crash".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
