use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::UserIdExtension;

/// Middleware that requires an authenticated user (UserIdExtension must be present and non-empty).
pub async fn authenticated_user_guard(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user_id = request
        .extensions()
        .get::<UserIdExtension>()
        .map(|e| e.0.as_str())
        .unwrap_or("");

    if user_id.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use tower::ServiceExt;

    async fn protected_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_auth_guard_no_extension() {
        let app = Router::new()
            .route("/test", axum::routing::get(protected_handler))
            .layer(axum::middleware::from_fn(authenticated_user_guard));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_guard_empty_user_id() {
        let app = Router::new()
            .route("/test", axum::routing::get(protected_handler))
            .layer(axum::middleware::from_fn(authenticated_user_guard));

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(UserIdExtension(String::new()));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_guard_valid_user_id() {
        let app = Router::new()
            .route("/test", axum::routing::get(protected_handler))
            .layer(axum::middleware::from_fn(authenticated_user_guard));

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(UserIdExtension("alice".into()));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
