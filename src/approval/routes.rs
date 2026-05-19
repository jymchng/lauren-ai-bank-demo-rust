use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use injectable::prelude::*;

use crate::approval::service::ApprovalService;
use crate::error::AppError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ApprovalRequest {
    #[serde(alias = "conversation_id")]
    pub approval_id: String,
    pub approved: bool,
    pub user_id: Option<String>,
}

pub async fn respond(
    approval: Inject<ApprovalService>,
    Json(body): Json<ApprovalRequest>,
) -> Result<Json<Value>, AppError> {
    match approval.respond(&body.approval_id, body.approved).await {
        Ok(()) => Ok(Json(json!({"status": "ok"}))),
        Err(e) => Err(AppError::BadRequest(e)),
    }
}

pub fn approval_router() -> axum::Router<AppState> {
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
