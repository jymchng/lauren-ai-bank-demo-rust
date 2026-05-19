pub mod schemas;
pub mod sse;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use agtrs::prelude::*;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures::StreamExt;
use serde_json::json;

use crate::chat::schemas::ChatRequest;
use crate::chat::sse::stream_event_to_sse;
use crate::AppState;

/// SSE streaming chat endpoint (authenticated — user_id required in body).
pub async fn stream_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let Some(user_id) = req.user_id.clone() else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "user_id required"})),
        )
            .into_response();
    };

    let conv_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let event_stream = build_chat_stream(state, req.message, conv_id, Some(user_id));
    Sse::new(Box::pin(event_stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// SSE streaming chat endpoint (public — no authentication required).
pub async fn stream_chat_public(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let conv_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let event_stream = build_chat_stream(state, req.message, conv_id, None);
    Sse::new(Box::pin(event_stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Build a multi-agent streaming response with handoff support.
fn build_chat_stream(
    state: Arc<AppState>,
    message: String,
    conversation_id: String,
    user_id: Option<String>,
) -> impl futures::Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let (tx, rx) = futures::channel::mpsc::unbounded::<Result<Event, Infallible>>();
    let approval_svc = Arc::clone(&state.approval_service);
    let conv_id_cleanup = conversation_id.clone();

    tokio::spawn(async move {
        let max_handoffs: usize = if user_id.is_some() { 8 } else { 4 };

        let mut current_agent_name = state
            .active_agent_store
            .get_active_agent(&conversation_id)
            .await
            .unwrap_or_else(|| {
                if user_id.is_some() {
                    "authenticated_crm".to_string()
                } else {
                    "unauthenticated_crm".to_string()
                }
            });

        // Load conversation history
        let history: Vec<Message> = {
            let store = state.conversation_history.read().await;
            store.get(&conversation_id).cloned().unwrap_or_default()
        };

        let mut current_message = message;

        'handoff: for _ in 0..max_handoffs {
            let agent = match state.get_agent_by_name(&current_agent_name) {
                Some(a) => a,
                None => break,
            };

            // Prepend context summary if a handoff just happened
            let summary = state
                .active_agent_store
                .get_and_clear_summary(&conversation_id)
                .await;
            let input_text = match summary {
                Some(s) => {
                    format!(
                        "[Context from previous agent]: {s}\n\n[User message]: {current_message}"
                    )
                }
                None => current_message.clone(),
            };

            // Build AgentContext with user/conversation state
            let mut ctx = AgentContext::new(
                &current_agent_name,
                agent.config().clone(),
                Arc::clone(&state.llm),
                Arc::clone(&state.resolve_ctx),
            );

            let mut ctx_state = HashMap::new();
            ctx_state.insert(
                "conversation_id".to_string(),
                serde_json::Value::String(conversation_id.clone()),
            );
            if let Some(ref uid) = user_id {
                ctx_state.insert(
                    "user_id".to_string(),
                    serde_json::Value::String(uid.clone()),
                );
            }
            ctx.set_context_state(ctx_state);

            // Seed context with prior conversation history
            for msg in &history {
                ctx.add_message(msg.clone());
            }

            // Stream this agent's turn (executor auto-registers agent.tools())
            let mut agent_stream =
                AgentExecutor::run_stream(Arc::clone(&agent), Message::user(input_text), ctx);

            let mut done = false;
            while let Some(event) = agent_stream.next().await {
                if matches!(&event, StreamEvent::Done { .. }) {
                    done = true;
                }
                if let Some(sse_event) = stream_event_to_sse(&event) {
                    let data = serde_json::to_string(&sse_event).unwrap_or_default();
                    let _ = tx.unbounded_send(Ok(Event::default().data(data)));
                }
            }

            if !done {
                break;
            }

            // Check for agent handoff
            let new_agent = state
                .active_agent_store
                .get_active_agent(&conversation_id)
                .await;
            match new_agent {
                Some(name) if name != current_agent_name => {
                    use crate::chat::schemas::SseEvent;
                    let handoff = SseEvent::AgentHandoff {
                        from: current_agent_name.clone(),
                        to: name.clone(),
                        summary: String::new(),
                    };
                    let data = serde_json::to_string(&handoff).unwrap_or_default();
                    let _ = tx.unbounded_send(Ok(Event::default().data(data)));
                    current_agent_name = name;
                }
                _ => break 'handoff,
            }
        }

        // Cleanup: cancel any pending approvals when the stream ends / client disconnects
        approval_svc.cancel_approval(&conv_id_cleanup).await;
    });

    rx
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
    use crate::chat::schemas::{ChatRequest, SseEvent};
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[test]
    fn test_sse_event_serialization() {
        let event = SseEvent::TextDelta {
            delta: "Hello".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("text_delta"));

        let event = SseEvent::Done {
            content: "Done".into(),
            conversation_id: "conv-1".into(),
            turns: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("done"));
    }

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"message": "hello", "conversation_id": "conv1"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert_eq!(req.conversation_id, Some("conv1".into()));
    }

    #[test]
    fn test_chat_request_optional_fields() {
        let json = r#"{"message": "hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert!(req.conversation_id.is_none());
        assert!(req.user_id.is_none());
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
    async fn test_chat_authenticated_endpoint_without_user_id() {
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
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_chat_authenticated_endpoint_with_user_id() {
        let app = create_test_app().await;
        let body = serde_json::json!({"message": "What's my balance?", "user_id": "alice", "conversation_id": "test-auth"});
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
    async fn test_stream_chat_public_direct() {
        let state = crate::test_utils::create_test_state().await;
        let req = ChatRequest {
            message: "Hello".into(),
            conversation_id: None,
            user_id: None,
        };
        let result = stream_chat_public(axum::extract::State(state), axum::Json(req)).await;
        let response = result.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
