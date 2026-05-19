pub mod schemas;
pub mod sse;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use agtrs::prelude::*;
use agtrs_runtime::memory::InMemoryConversationStore;
use agtrs_runtime::signals as agtrs_sig;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures::StreamExt;
use injectable::axum::InjectableState;
use injectable::prelude::*;
use serde_json::json;

use crate::agents::active_agent_store::ActiveAgentStore;
use crate::agents::auth_crm::AuthenticatedCrmAgent;
use crate::agents::disputes::DisputesAgent;
use crate::agents::transfer::BankTransferAgent;
use crate::agents::unauth_crm::UnauthenticatedCrmAgent;
use crate::approval::service::ApprovalService;
use crate::chat::schemas::ChatRequest;
use crate::chat::sse::stream_event_to_sse;
use crate::signals::bus::AppSignal;
use crate::signals::bus::AppSignalBus;
use crate::AppState;

/// Captures the full `axum::http::Extensions` map from request parts so middleware-injected
/// typed values (e.g. auth tokens) propagate into tool execution.
struct RequestExtensions(axum::http::Extensions);

#[async_trait::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for RequestExtensions {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(RequestExtensions(parts.extensions.clone()))
    }
}

/// All dependencies needed by the chat stream loop — built once in the handler
/// and moved into the spawned task.
struct ChatDeps {
    agents: HashMap<String, Arc<dyn Agent>>,
    active_agent_store: Arc<ActiveAgentStore>,
    approval_service: Arc<ApprovalService>,
    signal_bus: Arc<AppSignalBus>,
    conv_store: Arc<InMemoryConversationStore>,
    llm: Arc<dyn LlmProvider>,
    resolve_ctx: Arc<injectable_runtime::ResolveContext>,
    extensions: axum::http::Extensions,
}

/// SSE streaming chat endpoint (authenticated — user_id required in body).
pub async fn stream_chat(
    State(state): State<AppState>,
    RequestExtensions(mut extensions): RequestExtensions,
    store: Inject<ActiveAgentStore>,
    approval: Inject<ApprovalService>,
    bus: Inject<AppSignalBus>,
    conv_store: Inject<InMemoryConversationStore>,
    unauth: Inject<UnauthenticatedCrmAgent>,
    auth: Inject<AuthenticatedCrmAgent>,
    transfer: Inject<BankTransferAgent>,
    disputes: Inject<DisputesAgent>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let Some(user_id) = req.user_id.clone() else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "user_id required"})),
        )
            .into_response();
    };

    extensions.insert(crate::error::UserIdExtension(user_id.clone()));

    let conv_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let deps = build_deps(
        state, extensions, store, approval, bus, conv_store, unauth, auth, transfer, disputes,
    );
    let event_stream = build_chat_stream(deps, req.last_user_message(), conv_id, Some(user_id));
    Sse::new(Box::pin(event_stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// SSE streaming chat endpoint (public — no authentication required).
pub async fn stream_chat_public(
    State(state): State<AppState>,
    RequestExtensions(extensions): RequestExtensions,
    store: Inject<ActiveAgentStore>,
    approval: Inject<ApprovalService>,
    bus: Inject<AppSignalBus>,
    conv_store: Inject<InMemoryConversationStore>,
    unauth: Inject<UnauthenticatedCrmAgent>,
    auth: Inject<AuthenticatedCrmAgent>,
    transfer: Inject<BankTransferAgent>,
    disputes: Inject<DisputesAgent>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let conv_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let deps = build_deps(
        state, extensions, store, approval, bus, conv_store, unauth, auth, transfer, disputes,
    );
    let event_stream = build_chat_stream(deps, req.last_user_message(), conv_id, None);
    Sse::new(Box::pin(event_stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn build_deps(
    state: AppState,
    extensions: axum::http::Extensions,
    store: Inject<ActiveAgentStore>,
    approval: Inject<ApprovalService>,
    bus: Inject<AppSignalBus>,
    conv_store: Inject<InMemoryConversationStore>,
    unauth: Inject<UnauthenticatedCrmAgent>,
    auth: Inject<AuthenticatedCrmAgent>,
    transfer: Inject<BankTransferAgent>,
    disputes: Inject<DisputesAgent>,
) -> ChatDeps {
    let agents: HashMap<String, Arc<dyn Agent>> = [
        (
            "Banking CRM Agent (Public)",
            Arc::clone(&unauth.0) as Arc<dyn Agent>,
        ),
        (
            "Banking CRM Agent (Authenticated)",
            Arc::clone(&auth.0) as Arc<dyn Agent>,
        ),
        (
            "Banking Transfer Agent",
            Arc::clone(&transfer.0) as Arc<dyn Agent>,
        ),
        (
            "Banking Disputes Agent",
            Arc::clone(&disputes.0) as Arc<dyn Agent>,
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let resolve_ctx = Arc::new(state.resolve_context().clone());

    ChatDeps {
        agents,
        active_agent_store: Arc::clone(&store.0),
        approval_service: Arc::clone(&approval.0),
        signal_bus: Arc::clone(&bus.0),
        conv_store: Arc::clone(&conv_store.0),
        llm: Arc::clone(&state.llm),
        resolve_ctx,
        extensions,
    }
}

/// Build a multi-agent streaming response with handoff support.
fn build_chat_stream(
    deps: ChatDeps,
    message: String,
    conversation_id: String,
    user_id: Option<String>,
) -> impl futures::Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let (tx, rx) = futures::channel::mpsc::unbounded::<Result<Event, Infallible>>();
    let approval_svc = Arc::clone(&deps.approval_service);
    let conv_id_cleanup = conversation_id.clone();

    tokio::spawn(async move {
        let max_handoffs: usize = if user_id.is_some() { 8 } else { 4 };

        let mut current_agent_name = deps
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
            let ws_tx = deps.signal_bus.sender();
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
            let ws_tx = deps.signal_bus.sender();
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
            let ws_tx = deps.signal_bus.sender();
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
            let ws_tx = deps.signal_bus.sender();
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

        let mut accumulated_history: Vec<Message> = deps
            .conv_store
            .load(&conversation_id)
            .await
            .unwrap_or_default();

        let current_message = message;
        let mut is_first_turn = true;
        let mut pending_input_summary: Option<String> = None;

        'handoff: for _ in 0..max_handoffs {
            let agent = match deps.agents.get(&current_agent_name) {
                Some(a) => Arc::clone(a),
                None => break,
            };

            let _ = tx.unbounded_send(Ok(Event::default()
                .event("agent_started")
                .data(current_agent_name.clone())));

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

            let ctx = AgentContext::builder(
                &current_agent_name,
                agent.config().clone(),
                Arc::clone(&deps.llm),
                Arc::clone(&deps.resolve_ctx),
            )
            .with_signals(Arc::clone(&per_request_signals))
            .with_conversation_store(Arc::clone(&deps.conv_store) as Arc<dyn ConversationStore>)
            .with_history(accumulated_history.iter().cloned())
            .with_state(ctx_state)
            .with_extensions(deps.extensions.clone())
            .build();

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

            let new_agent = deps
                .active_agent_store
                .get_active_agent(&conversation_id)
                .await;
            match new_agent {
                Some(name) if name != current_agent_name => {
                    let handoff_summary = deps
                        .active_agent_store
                        .get_and_clear_summary(&conversation_id)
                        .await
                        .unwrap_or_default();
                    if !handoff_summary.is_empty() {
                        pending_input_summary = Some(handoff_summary.clone());
                    }
                    deps.signal_bus.emit(AppSignal::AgentHandoff {
                        from_agent: current_agent_name.clone(),
                        to_agent: name.clone(),
                        summary: handoff_summary,
                        conversation_id: conversation_id.clone(),
                    });
                    let _ =
                        tx.unbounded_send(Ok(Event::default().event("break").data(name.clone())));
                    current_agent_name = name;
                }
                _ => break 'handoff,
            }
        }

        let _ = deps
            .conv_store
            .save(&conversation_id, &accumulated_history)
            .await;

        let _ = tx.unbounded_send(Ok(Event::default().event("done").data("")));

        approval_svc.cancel_approval(&conv_id_cleanup).await;
    });

    rx
}

pub fn chat_router() -> axum::Router<AppState> {
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
}
