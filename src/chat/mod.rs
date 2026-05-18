use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

/// Chat request body.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<String>,
}

/// Chat response for SSE events.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    /// A token delta from the agent.
    #[serde(rename = "token")]
    Token { content: String },
    /// A tool was used.
    #[serde(rename = "tool_use")]
    ToolUse { tool: String },
    /// A handoff occurred.
    #[serde(rename = "break")]
    Break,
    /// Agent finished.
    #[serde(rename = "done")]
    Done { turns: usize, tool_calls: usize },
    /// An error occurred.
    #[serde(rename = "error")]
    Error { error: String },
    /// Guardrail modified the response.
    #[serde(rename = "guardrail_override")]
    GuardrailOverride { message: String },
}

/// SSE streaming chat endpoint (authenticated).
pub async fn stream_chat(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ChatRequest>,
) -> impl IntoResponse {
    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // For now, return a simple response. Full agent orchestration requires
    // the LLM provider which is not available in test/dev mode.
    let response = json!({
        "status": "ok",
        "conversation_id": conversation_id,
        "message": "Chat endpoint received. Agent orchestration requires LLM provider configuration."
    });

    axum::Json(response)
}

/// SSE streaming chat endpoint (public).
pub async fn stream_chat_public(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ChatRequest>,
) -> impl IntoResponse {
    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let response = json!({
        "status": "ok",
        "conversation_id": conversation_id,
        "message": "Public chat endpoint received."
    });

    axum::Json(response)
}

/// Create the chat router.
pub fn chat_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/banking/chat", axum::routing::post(stream_chat))
        .route(
            "/api/banking/chat/public",
            axum::routing::post(stream_chat_public),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_chat_event_serialization() {
        let token_event = ChatEvent::Token {
            content: "Hello".into(),
        };
        let json = serde_json::to_string(&token_event).unwrap();
        assert!(json.contains("token"));

        let tool_event = ChatEvent::ToolUse {
            tool: "get_balance".into(),
        };
        let json = serde_json::to_string(&tool_event).unwrap();
        assert!(json.contains("tool_use"));

        let done_event = ChatEvent::Done {
            turns: 3,
            tool_calls: 1,
        };
        let json = serde_json::to_string(&done_event).unwrap();
        assert!(json.contains("done"));

        let error_event = ChatEvent::Error {
            error: "failed".into(),
        };
        let json = serde_json::to_string(&error_event).unwrap();
        assert!(json.contains("error"));

        let guardrail_event = ChatEvent::GuardrailOverride {
            message: "redirected".into(),
        };
        let json = serde_json::to_string(&guardrail_event).unwrap();
        assert!(json.contains("guardrail_override"));

        let break_event = ChatEvent::Break;
        let json = serde_json::to_string(&break_event).unwrap();
        assert!(json.contains("break"));
    }

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"message": "hello", "conversation_id": "conv1"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert_eq!(req.conversation_id, Some("conv1".into()));
    }

    #[test]
    fn test_chat_request_optional_conversation_id() {
        let json = r#"{"message": "hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert!(req.conversation_id.is_none());
    }

    #[tokio::test]
    async fn test_chat_public_endpoint() {
        let app = create_test_app().await;
        let body = serde_json::json!({"message": "Hello", "conversation_id": "test-conv"});
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/chat/public")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chat_authenticated_endpoint() {
        let app = create_test_app().await;
        let body = serde_json::json!({"message": "What's my balance?"});
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/banking/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_stream_chat_direct() {
        let state = crate::test_utils::create_test_state().await;
        let req = ChatRequest {
            message: "Hello".into(),
            conversation_id: Some("test-conv".into()),
        };
        let result = stream_chat(axum::extract::State(state), axum::Json(req)).await;
        let response = result.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_stream_chat_public_direct() {
        let state = crate::test_utils::create_test_state().await;
        let req = ChatRequest {
            message: "Hello".into(),
            conversation_id: None,
        };
        let result = stream_chat_public(axum::extract::State(state), axum::Json(req)).await;
        let response = result.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
