use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use injectable::prelude::*;

use crate::crypto::service::CryptoService;
use crate::error::UserIdExtension;
use crate::AppState;

/// Middleware that verifies the X-Signature HMAC header and extracts user_id.
pub async fn signature_guard(
    crypto: Inject<CryptoService>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
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

    if !crypto.verify(&bytes, &signature) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let user_id = parsed
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut new_request = Request::from_parts(parts, Body::from(bytes));
    new_request
        .extensions_mut()
        .insert(UserIdExtension(user_id));

    Ok(next.run(new_request).await)
}

mod tests {
    use super::*;
    use crate::crypto::service::CryptoService;
    use crate::test_utils::create_test_state;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use std::sync::Arc;
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
        let state = create_test_state().await;
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
        let state = create_test_state().await;
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
        let state = create_test_state().await;
        let body = r#"{"user_id":"alice"}"#;
        let crypto: Arc<CryptoService> = state.container().resolve_external().await.unwrap();
        let signature = crypto.sign(body.as_bytes());

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
