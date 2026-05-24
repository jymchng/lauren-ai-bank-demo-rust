//! WebSocket token REST routes.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::AppError;
use crate::AppState;

/// WS token request (authenticated).
#[derive(Debug, Deserialize)]
pub struct WsTokenRequest {
    /// The user ID for the token.
    pub user_id: String,
}

/// WS token response.
#[derive(Debug, Serialize)]
pub struct WsTokenResponse {
    /// The generated token.
    pub token: String,
    /// Token TTL in seconds.
    pub ttl_seconds: i64,
}

/// POST /api/banking/ws-token — create a WS token for an authenticated user.
pub async fn create_ws_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WsTokenRequest>,
) -> Result<Json<WsTokenResponse>, AppError> {
    let token = state.ws_token_service.create_token(&req.user_id);
    Ok(Json(WsTokenResponse {
        token,
        ttl_seconds: 120,
    }))
}

/// POST /api/banking/ws-token/public — create a WS token for anonymous access.
pub async fn create_public_ws_token(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WsTokenResponse>, AppError> {
    let token = state.ws_token_service.create_public_token();
    Ok(Json(WsTokenResponse {
        token,
        ttl_seconds: 120,
    }))
}

mod tests {
    use super::*;

    #[test]
    fn test_ws_token_request_deserialization() {
        let json = r#"{"user_id":"user-123"}"#;
        let req: WsTokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.user_id, "user-123");
    }

    #[test]
    fn test_ws_token_response_serialization() {
        let resp = WsTokenResponse {
            token: "abc123".into(),
            ttl_seconds: 120,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("120"));
    }
}
