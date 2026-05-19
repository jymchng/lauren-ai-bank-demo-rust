use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::approval::service::ApprovalService;
use crate::error::AppError;
use crate::AppState;

/// Request body for the approval endpoint.
/// Accepts `approval_id` (Python-compatible) or `conversation_id` as the key.
#[derive(Debug, Deserialize)]
pub struct ApprovalRequest {
    /// Python frontend sends this as `approval_id` (== conversation_id in Rust).
    #[serde(alias = "conversation_id")]
    pub approval_id: String,
    pub approved: bool,
    /// Ignored — user_id is verified by the approval service's stored record.
    pub user_id: Option<String>,
}

/// Handle an approval response from the browser.
pub async fn respond(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ApprovalRequest>,
) -> Result<Json<Value>, AppError> {
    match state
        .approval_service
        .respond(&body.approval_id, body.approved)
        .await
    {
        Ok(()) => Ok(Json(json!({"status": "ok"}))),
        Err(e) => Err(AppError::BadRequest(e)),
    }
}

/// Create the approval router.
pub fn approval_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route("/api/banking/approval", axum::routing::post(respond))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_approval_endpoint_no_pending() {
        let app = create_test_app().await;
        let body = serde_json::json!({
            "approval_id": "nonexistent",
            "approved": true
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/approval")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
