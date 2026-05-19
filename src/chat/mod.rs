pub mod schemas;
pub mod sse;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use agtrs::prelude::*;
use agtrs_runtime::signals as agtrs_sig;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures::StreamExt;
use serde_json::json;

use crate::chat::schemas::ChatRequest;
use crate::chat::sse::stream_event_to_sse;
use crate::signals::bus::AppSignal;
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

    let event_stream = build_chat_stream(state, req.last_user_message(), conv_id, Some(user_id));
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

    let event_stream = build_chat_stream(state, req.last_user_message(), conv_id, None);
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
                    "Banking CRM Agent (Authenticated)".to_string()
                } else {
                    "Banking CRM Agent (Public)".to_string()
                }
            });

        // Per-request signal bus — bridges executor-native signals to the shared WS broadcast
        // with the current conversation_id injected so the frontend can route them.
        let per_request_signals = Arc::new(agtrs_runtime::signals::SignalBus::new());
        {
            let conv_id = conversation_id.clone();
            let ws_tx = state.signal_bus.sender();
            per_request_signals
                .on::<agtrs_sig::ToolCallStarted>(move |e| {
                    let _ = ws_tx.send(AppSignal::ToolCallStarted {
                        tool_name: e.tool_name.clone(),
                        tool_use_id: e.tool_use_id.clone(),
                        conversation_id: conv_id.clone(),
                    });
                })
                .await;

            let conv_id = conversation_id.clone();
            let ws_tx = state.signal_bus.sender();
            per_request_signals
                .on::<agtrs_sig::ToolCallComplete>(move |e| {
                    let _ = ws_tx.send(AppSignal::ToolCallComplete {
                        tool_name: e.tool_name.clone(),
                        tool_use_id: e.tool_use_id.clone(),
                        duration_ms: e.duration_ms as u64,
                        success: e.success,
                        error: e.error.clone(),
                        conversation_id: conv_id.clone(),
                    });
                })
                .await;

            let conv_id = conversation_id.clone();
            let ws_tx = state.signal_bus.sender();
            per_request_signals
                .on::<agtrs_sig::ModelCallComplete>(move |e| {
                    let _ = ws_tx.send(AppSignal::ModelCallComplete {
                        model: e.model.clone(),
                        input_tokens: e.usage.input_tokens,
                        output_tokens: e.usage.output_tokens,
                        cost_usd: e.cost_usd,
                        duration_ms: e.duration_ms as u64,
                        conversation_id: conv_id.clone(),
                    });
                })
                .await;

            let conv_id = conversation_id.clone();
            let ws_tx = state.signal_bus.sender();
            per_request_signals
                .on::<agtrs_sig::AgentRunComplete>(move |e| {
                    let _ = ws_tx.send(AppSignal::AgentRunComplete {
                        agent_name: e.agent_name.clone(),
                        turns: e.turns,
                        total_cost_usd: e.total_cost_usd,
                        conversation_id: conv_id.clone(),
                    });
                })
                .await;
        }

        // Load conversation history from the shared store.
        let mut accumulated_history: Vec<Message> = state
            .conv_store
            .load(&conversation_id)
            .await
            .unwrap_or_default();

        let current_message = message;
        let mut is_first_turn = true;
        // Carry the handoff summary across loop iterations so it can be emitted in
        // AgentHandoff (detected at end of current iter) AND fed as input to the next agent.
        let mut pending_input_summary: Option<String> = None;

        'handoff: for _ in 0..max_handoffs {
            let agent = match state.get_agent_by_name(&current_agent_name) {
                Some(a) => a,
                None => break,
            };

            // Emit agent_started so the client always knows who is speaking.
            let _ = tx.unbounded_send(Ok(Event::default()
                .event("agent_started")
                .data(current_agent_name.clone())));

            // First turn: original user message; subsequent turns: handoff summary.
            let input_text = if is_first_turn {
                is_first_turn = false;
                current_message.clone()
            } else {
                match pending_input_summary.take() {
                    Some(s) => s,
                    None => current_message.clone(),
                }
            };

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

            // Build AgentContext via builder — injects shared signal bus and conv store.
            let ctx = AgentContext::builder(
                &current_agent_name,
                agent.config().clone(),
                Arc::clone(&state.llm),
                Arc::clone(&state.resolve_ctx),
            )
            .with_signals(Arc::clone(&per_request_signals))
            .with_conversation_store(Arc::clone(&state.conv_store) as Arc<dyn ConversationStore>)
            .with_history(accumulated_history.iter().cloned())
            .with_state(ctx_state)
            .build();

            // Stream this agent's turn (executor auto-registers agent.tools()).
            let mut agent_stream =
                AgentExecutor::run_stream(Arc::clone(&agent), Message::user(input_text), ctx);

            let mut done = false;
            while let Some(event) = agent_stream.next().await {
                if let StreamEvent::Done { messages, .. } = &event {
                    accumulated_history = messages.clone();
                    done = true;
                }
                if let Some(sse_event) = stream_event_to_sse(&event) {
                    let _ = tx.unbounded_send(Ok(sse_event));
                }
            }

            if !done {
                break;
            }

            // Check for agent handoff.
            let new_agent = state
                .active_agent_store
                .get_active_agent(&conversation_id)
                .await;
            match new_agent {
                Some(name) if name != current_agent_name => {
                    let handoff_summary = state
                        .active_agent_store
                        .get_and_clear_summary(&conversation_id)
                        .await
                        .unwrap_or_default();
                    if !handoff_summary.is_empty() {
                        pending_input_summary = Some(handoff_summary.clone());
                    }
                    // Emit agent_handoff WS event so the frontend can update "Talking to:".
                    state.signal_bus.emit(AppSignal::AgentHandoff {
                        from_agent: current_agent_name.clone(),
                        to_agent: name.clone(),
                        summary: handoff_summary,
                        conversation_id: conversation_id.clone(),
                    });
                    // Python `break` event: data is the target agent name.
                    let _ =
                        tx.unbounded_send(Ok(Event::default().event("break").data(name.clone())));
                    current_agent_name = name;
                }
                _ => break 'handoff,
            }
        }

        // Persist conversation history.
        let _ = state
            .conv_store
            .save(&conversation_id, &accumulated_history)
            .await;

        // Emit final done — once for the entire multi-agent run (not per-agent turn).
        let _ = tx.unbounded_send(Ok(Event::default().event("done").data("")));

        // Cleanup: cancel any pending approvals when the stream ends / client disconnects.
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
    use crate::chat::schemas::{ChatMessage, ChatRequest};
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn make_req(content: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: content.into(),
            }],
            model: None,
            conversation_id: None,
            user_id: None,
        }
    }

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"messages":[{"role":"user","content":"hello"}],"conversation_id":"conv1"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.last_user_message(), "hello");
        assert_eq!(req.conversation_id, Some("conv1".into()));
    }

    #[test]
    fn test_chat_request_optional_fields() {
        let json = r#"{"messages":[{"role":"user","content":"hello"}]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.last_user_message(), "hello");
        assert!(req.conversation_id.is_none());
        assert!(req.user_id.is_none());
    }

    #[tokio::test]
    async fn test_chat_public_endpoint() {
        let app = create_test_app().await;
        let body = serde_json::json!({"messages":[{"role":"user","content":"Hello"}],"conversation_id":"test-conv"});
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
        let body = serde_json::json!({"messages":[{"role":"user","content":"What's my balance?"}]});
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
        let body = serde_json::json!({"messages":[{"role":"user","content":"What's my balance?"}],"user_id":"alice","conversation_id":"test-auth"});
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
        let req = make_req("Hello");
        let result = stream_chat_public(axum::extract::State(state), axum::Json(req)).await;
        let response = result.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
