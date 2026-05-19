use axum::Json;
use serde_json::{json, Value};

/// Health check endpoint.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Create the health router.
pub fn health_router() -> axum::Router<crate::AppState> {
    axum::Router::new().route("/api/health", axum::routing::get(health))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
