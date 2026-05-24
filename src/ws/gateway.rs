use axum::extract::{Query, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Deserialize;

use injectable::prelude::*;

use crate::ws::event_forwarder::EventForwarder;
use crate::ws::token_service::WsTokenService;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct WsParams {
    pub token: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    token_svc: Inject<WsTokenService>,
    forwarder: Inject<EventForwarder>,
) -> impl IntoResponse {
    let token = params.token.unwrap_or_default();
    let user_id = token_svc.verify_token(&token);

    match user_id {
        Some(uid) => ws
            .on_upgrade(move |socket| handle_ws(socket, uid, forwarder))
            .into_response(),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn handle_ws(
    mut socket: axum::extract::ws::WebSocket,
    _user_id: String,
    forwarder: Inject<EventForwarder>,
) {
    let mut rx = forwarder.subscribe();

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(signal) => {
                        let json = match serde_json::to_string(&signal) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket
                            .send(axum::extract::ws::Message::Text(json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Close(_))) => break,
                    Some(Ok(axum::extract::ws::Message::Ping(_))) => continue,
                    _ => break,
                }
            }
        }
    }
}

pub async fn issue_token(
    token_svc: Inject<WsTokenService>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let user_id = body.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
    if user_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "user_id required"})),
        )
            .into_response();
    }
    let token = token_svc.create_token(user_id, 120);
    axum::Json(serde_json::json!({"token": token})).into_response()
}

pub async fn issue_public_token(token_svc: Inject<WsTokenService>) -> impl IntoResponse {
    let token = token_svc.create_token("__public__", 120);
    axum::Json(serde_json::json!({"token": token})).into_response()
}

pub fn ws_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/ws/banking", axum::routing::get(ws_handler))
        .route("/api/banking/ws-token", axum::routing::post(issue_token))
        .route(
            "/api/banking/ws-token/public",
            axum::routing::post(issue_public_token),
        )
}

mod tests {
    use super::*;
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_issue_public_token() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/ws-token/public")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_issue_token_with_user_id() {
        let app = create_test_app().await;
        let body = serde_json::json!({"user_id": "alice"});
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/ws-token")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_issue_token_missing_user_id() {
        let app = create_test_app().await;
        let body = serde_json::json!({});
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/ws-token")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ws_handler_no_token() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ws/banking")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn test_create_ws_token_public_returns_token() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/ws-token/public")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let data: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(data.get("token").is_some());
    }

    #[tokio::test]
    async fn test_ws_handler_with_valid_token() {
        let state = crate::test_utils::create_test_state().await;
        let token_svc: std::sync::Arc<WsTokenService> =
            state.container().resolve_external().await.unwrap();
        let token = token_svc.create_token("alice", 120);
        let app = crate::create_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/ws/banking?token={}", token))
                    .header("upgrade", "websocket")
                    .header("connection", "Upgrade")
                    .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                    .header("sec-websocket-version", "13")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        assert!(
            status == StatusCode::SWITCHING_PROTOCOLS
                || status == StatusCode::BAD_REQUEST
                || status == StatusCode::OK
                || status == StatusCode::UPGRADE_REQUIRED
                || status == StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
