use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::crypto::service::CryptoService;
use crate::error::UserIdExtension;
use crate::AppState;

/// Middleware that verifies the X-Signature HMAC header and extracts user_id.
pub async fn signature_guard(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Clone the header before consuming the body
    let signature = request
        .headers()
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if !state.crypto_service.verify(&bytes, &signature) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Parse user_id from verified body
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let user_id = parsed
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Reconstruct request with body and user_id extension
    let mut new_request = Request::from_parts(parts, Body::from(bytes));
    new_request
        .extensions_mut()
        .insert(UserIdExtension(user_id));

    Ok(next.run(new_request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_state;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use tower::ServiceExt;

    async fn protected_handler(request: Request<Body>) -> &'static str {
        let user_id = request
            .extensions()
            .get::<UserIdExtension>()
            .map(|e| e.0.as_str())
            .unwrap_or("none");
        if user_id == "alice" {
            "authenticated"
        } else {
            "no-user"
        }
    }

    #[tokio::test]
    async fn test_signature_guard_missing_header() {
        let state = crate::test_utils::create_test_state().await;
        let app = Router::new()
            .route("/test", axum::routing::post(protected_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                signature_guard,
            ))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"user_id":"alice"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_signature_guard_invalid_signature() {
        let state = crate::test_utils::create_test_state().await;
        let body = r#"{"user_id":"alice"}"#;
        let app = Router::new()
            .route("/test", axum::routing::post(protected_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                signature_guard,
            ))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("X-Signature", "invalid-signature")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_signature_guard_valid_signature() {
        let state = crate::test_utils::create_test_state().await;
        let body = r#"{"user_id":"alice"}"#;
        let signature = state.crypto_service.sign(body.as_bytes());

        let app = Router::new()
            .route("/test", axum::routing::post(protected_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                signature_guard,
            ))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("X-Signature", &signature)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
