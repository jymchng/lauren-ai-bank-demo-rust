//! End-to-end integration tests for the lauren-chatbot API.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use futures::StreamExt;
use tower::ServiceExt;

use agtrs::agtrs_runtime::testing::{MockLlmProvider, MockTransport};
use lauren_chatbot::{build_test_state, create_router, AppState};

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn create_test_app() -> Router {
    let state = build_test_state().await;
    create_router(state)
}

/// Build app with a mock LLM so agent execution doesn't hit a real API.
async fn create_mock_llm_app() -> (Router, Arc<MockTransport>) {
    let transport = Arc::new(MockTransport::new());
    let mock_llm = Arc::new(MockLlmProvider::new(Arc::clone(&transport)));

    let state = build_test_state().await.with_llm(mock_llm);

    (create_router(state), transport)
}

/// Consume the full SSE body and return all `data:` payloads as parsed JSON values.
async fn collect_sse_events(response: axum::response::Response) -> Vec<serde_json::Value> {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read SSE body");

    let text = String::from_utf8_lossy(&bytes);
    text.split("\n\n")
        .filter_map(|block| {
            if block.trim().is_empty() {
                return None;
            }
            let event_type = block
                .lines()
                .find(|l| l.starts_with("event:"))?
                .trim_start_matches("event:")
                .trim()
                .to_string();
            let data = block
                .lines()
                .find(|l| l.starts_with("data:"))
                .map(|l| l.trim_start_matches("data:").trim().to_string())
                .unwrap_or_default();
            Some(serde_json::json!({"type": event_type, "data": data}))
        })
        .collect()
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

// ── Health ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health_check() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Banking REST ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_accounts_returns_non_empty_array() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/banking/accounts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["accounts"].is_array());
    assert!(!json["accounts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_known_account_alice() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/banking/accounts/alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"].as_str(), Some("alice"));
    assert!(json["balance"].as_f64().is_some());
}

#[tokio::test]
async fn test_get_unknown_account_returns_404() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/banking/accounts/nobody_xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_transactions_for_alice() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/banking/accounts/alice/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
}

// ── Metrics ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_metrics() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_cost() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/metrics/cost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_traces() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/metrics/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── WS Token ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_create_ws_token_public_returns_token() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/banking/ws-token/public")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["token"].is_string(), "expected token field");
}

// ── Chat SSE — status codes ───────────────────────────────────────────────────

#[tokio::test]
async fn test_chat_public_returns_200_with_sse_content_type() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"Hello"}],"conversation_id":"ct-1"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "expected SSE content-type, got: {ct}"
    );
}

#[tokio::test]
async fn test_chat_authenticated_without_user_id_returns_401() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({"messages":[{"role":"user","content":"hello"}]}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_chat_authenticated_with_user_id_returns_200_with_sse_content_type() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({"messages":[{"role":"user","content":"hello"}],"user_id":"alice","conversation_id":"ct-2"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "expected SSE content-type, got: {ct}"
    );
}

// ── Chat SSE — event content (mock LLM) ──────────────────────────────────────

#[tokio::test]
async fn test_chat_public_emits_text_delta_and_done() {
    let (app, transport) = create_mock_llm_app().await;
    transport.queue_text("Hello, I am Lauren!").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"Who are you?"}],"conversation_id":"ev-1"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let events = collect_sse_events(resp).await;
    assert!(!events.is_empty());
    assert!(
        events.iter().any(|e| e["type"] == "token"),
        "expected text_delta, got: {events:?}"
    );
    assert!(
        events.iter().any(|e| e["type"] == "done"),
        "expected done, got: {events:?}"
    );
}

#[tokio::test]
async fn test_chat_authenticated_emits_text_delta_and_done() {
    let (app, transport) = create_mock_llm_app().await;
    transport.queue_text("Your balance is $5,000.").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({"messages":[{"role":"user","content":"balance?"}],"user_id":"alice","conversation_id":"ev-2"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let events = collect_sse_events(resp).await;
    assert!(events.iter().any(|e| e["type"] == "token"));
    assert!(events.iter().any(|e| e["type"] == "done"));
}

#[tokio::test]
async fn test_chat_text_delta_accumulates_correct_content() {
    let (app, transport) = create_mock_llm_app().await;
    transport.queue_text("UniqueResponseMarker99").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"say the marker"}],"conversation_id":"ev-3"}),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    let full_text: String = events
        .iter()
        .filter(|e| e["type"] == "token")
        .filter_map(|e| e["data"].as_str())
        .collect();
    assert!(
        full_text.contains("UniqueResponseMarker99"),
        "expected response text in text_delta events, got: {full_text:?}"
    );
}

#[tokio::test]
async fn test_chat_done_event_has_positive_turns() {
    let (app, transport) = create_mock_llm_app().await;
    transport.queue_text("done").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"hi"}],"conversation_id":"ev-4"}),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    // done event is emitted (data is empty string in Python-compatible format)
    assert!(
        events.iter().any(|e| e["type"] == "done"),
        "expected done event, got: {events:?}"
    );
}

// ── Tool execution via SSE ────────────────────────────────────────────────────

#[tokio::test]
async fn test_chat_emits_tool_execution_and_tool_result_events() {
    let (app, transport) = create_mock_llm_app().await;

    // Turn 1: LLM calls get_balance
    transport
        .queue_tool_call("get_balance", serde_json::json!({"user_id": "alice"}))
        .await;
    // Turn 2: after tool result, LLM responds
    transport.queue_text("Your balance is $5,000.00.").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({
                "messages":[{"role":"user","content":"What is my balance?"}],
                "user_id": "alice",
                "conversation_id": "te-1"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let events = collect_sse_events(resp).await;
    assert!(
        events.iter().any(|e| e["type"] == "tool_use"),
        "expected tool_execution, got: {events:?}"
    );
    assert!(
        events.iter().any(|e| e["type"] == "done"),
        "expected done, got: {events:?}"
    );
}

#[tokio::test]
async fn test_chat_tool_execution_event_contains_tool_name() {
    let (app, transport) = create_mock_llm_app().await;
    transport
        .queue_tool_call("get_balance", serde_json::json!({"user_id": "alice"}))
        .await;
    transport.queue_text("Balance retrieved.").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({
                "messages":[{"role":"user","content":"my balance"}],
                "user_id": "alice",
                "conversation_id": "te-2"
            }),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    let te = events
        .iter()
        .find(|e| e["type"] == "tool_use")
        .expect("no tool_use event");
    // In Python-compatible format the tool name is the event data
    assert_eq!(te["data"].as_str(), Some("get_balance"));
}

#[tokio::test]
async fn test_chat_tool_use_event_emitted_before_done() {
    let (app, transport) = create_mock_llm_app().await;
    transport
        .queue_tool_call("get_balance", serde_json::json!({"user_id": "alice"}))
        .await;
    transport.queue_text("done.").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({
                "messages":[{"role":"user","content":"balance"}],
                "user_id": "alice",
                "conversation_id": "te-3"
            }),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    assert!(
        events.iter().any(|e| e["type"] == "tool_use"),
        "expected tool_use event, got: {events:?}"
    );
    assert!(
        events.iter().any(|e| e["type"] == "done"),
        "expected done event, got: {events:?}"
    );
}

#[tokio::test]
async fn test_chat_token_events_contain_balance_text() {
    let (app, transport) = create_mock_llm_app().await;
    transport
        .queue_tool_call("get_balance", serde_json::json!({"user_id": "alice"}))
        .await;
    transport.queue_text("Balance shown above.").await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat",
            serde_json::json!({
                "messages":[{"role":"user","content":"get balance"}],
                "user_id": "alice",
                "conversation_id": "te-4"
            }),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    let full_text: String = events
        .iter()
        .filter(|e| e["type"] == "token")
        .filter_map(|e| e["data"].as_str())
        .collect();
    assert!(
        !full_text.is_empty(),
        "expected token events after tool execution, got: {events:?}"
    );
}

// ── Error event when LLM is empty ─────────────────────────────────────────────

#[tokio::test]
async fn test_chat_emits_error_event_when_llm_has_no_responses() {
    let (app, _transport) = create_mock_llm_app().await;
    // No responses queued → MockTransport errors → executor emits Error event

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"hi"}],"conversation_id":"err-1"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK); // SSE always 200 immediately

    let events = collect_sse_events(resp).await;
    assert!(
        events.iter().any(|e| e["type"] == "error"),
        "expected error event when LLM queue is empty, got: {events:?}"
    );
}

// ── Approval workflow ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_approval_no_pending_returns_400() {
    let app = create_test_app().await;
    let resp = app
        .oneshot(post_json(
            "/api/banking/approval",
            serde_json::json!({"conversation_id": "no-such-conv", "approved": true}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_approval_workflow_approved() {
    use lauren_chatbot::approval::service::ApprovalService;
    let state = build_test_state().await;
    let approval_service: Arc<ApprovalService> =
        state.container().resolve_external().await.unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel();
    approval_service
        .create_pending_approval(
            "conv-approved",
            "alice",
            "transfer",
            serde_json::json!({"to_user": "bob", "amount": 100.0}),
            tx,
        )
        .await;

    assert!(approval_service.has_pending("conv-approved").await);

    let app = create_router(state);
    let resp = app
        .oneshot(post_json(
            "/api/banking/approval",
            serde_json::json!({"conversation_id": "conv-approved", "approved": true}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .expect("approval timed out")
        .expect("channel closed");
    assert!(decision);
}

#[tokio::test]
async fn test_approval_workflow_rejected() {
    use lauren_chatbot::approval::service::ApprovalService;
    let state = build_test_state().await;
    let approval_service: Arc<ApprovalService> =
        state.container().resolve_external().await.unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel();
    approval_service
        .create_pending_approval(
            "conv-rejected",
            "alice",
            "transfer",
            serde_json::json!({"to_user": "charlie", "amount": 50.0}),
            tx,
        )
        .await;

    let app = create_router(state);
    let resp = app
        .oneshot(post_json(
            "/api/banking/approval",
            serde_json::json!({"conversation_id": "conv-rejected", "approved": false}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .expect("rejection timed out")
        .expect("channel closed");
    assert!(!decision);
}

// ── Banking tools (resolved from DI container, user_id passed as input) ──────

#[tokio::test]
async fn test_get_balance_tool_returns_alice_balance() {
    use agtrs::prelude::*;
    use lauren_chatbot::tools::banking_tools::GetBalanceTool;

    let state = build_test_state().await;
    let tool: Arc<GetBalanceTool> = state.container().resolve_external().await.unwrap();
    let mut tool_ctx = ToolContext::new("tu-alice");
    tool_ctx
        .extensions
        .insert(lauren_chatbot::error::UserIdExtension("alice".to_string()));

    let result = tool
        .call(serde_json::json!({"user_id": "alice"}), &tool_ctx)
        .await
        .unwrap();
    assert!(
        !result.is_error,
        "expected success, got: {}",
        result.content
    );
    assert!(
        result.content.contains("5000"),
        "alice balance should be 5000, got: {}",
        result.content
    );
}

#[tokio::test]
async fn test_get_balance_tool_without_user_id_returns_error() {
    // Auth check lives in AuthRequiredHook — must use execute_with_hooks, not call().
    use agtrs::prelude::*;
    use agtrs_runtime::agent::{AgentConfig, AgentContext};
    use injectable_runtime::{EmptySingletonStore, ResolveContext};
    use lauren_chatbot::tools::banking_tools::GetBalanceTool;

    let state = build_test_state().await;
    let tool: Arc<GetBalanceTool> = state.container().resolve_external().await.unwrap();
    let tool_ctx = ToolContext::new("tu-noauth"); // no UserIdExtension

    let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
    let agent_ctx = AgentContext::new(
        "test",
        AgentConfig::default(),
        Arc::new(agtrs::agtrs_runtime::testing::MockLlmProvider::new(
            Arc::new(agtrs::agtrs_runtime::testing::MockTransport::new()),
        )),
        resolve_ctx,
    );

    let result = tool
        .execute_with_hooks(
            serde_json::json!({"user_id": "alice"}),
            &tool_ctx,
            &agent_ctx,
        )
        .await
        .unwrap();
    assert!(
        result.is_error,
        "expected error when no user_id in ctx state"
    );
}

#[tokio::test]
async fn test_get_balance_tool_for_unknown_user_returns_error() {
    use agtrs::prelude::*;
    use lauren_chatbot::tools::banking_tools::GetBalanceTool;

    let state = build_test_state().await;
    let tool: Arc<GetBalanceTool> = state.container().resolve_external().await.unwrap();
    let mut tool_ctx = ToolContext::new("tu-unknown");
    tool_ctx
        .extensions
        .insert(lauren_chatbot::error::UserIdExtension(
            "nobody_xyz".to_string(),
        ));

    let result = tool
        .call(serde_json::json!({"user_id": "nobody_xyz"}), &tool_ctx)
        .await
        .unwrap();
    assert!(result.is_error, "expected error for unknown user");
}

#[tokio::test]
async fn test_transfer_tool_requires_prior_approval() {
    use agtrs::prelude::*;
    use lauren_chatbot::tools::banking_tools::TransferFundsTool;

    let state = build_test_state().await;
    let tool: Arc<TransferFundsTool> = state.container().resolve_external().await.unwrap();
    let mut tool_ctx = ToolContext::new("tu-transfer");
    tool_ctx
        .extensions
        .insert(lauren_chatbot::error::UserIdExtension("alice".to_string()));

    let result = tool
        .call(
            serde_json::json!({"user_id": "alice", "to_user": "bob", "amount": 100.0}),
            &tool_ctx,
        )
        .await
        .unwrap();
    assert!(
        !result.is_error,
        "expected a message (not error) requiring approval"
    );
    assert!(
        result.content.to_lowercase().contains("approval")
            || result.content.to_lowercase().contains("approved"),
        "expected approval message, got: {}",
        result.content
    );
}

#[tokio::test]
async fn test_transfer_tool_succeeds_with_approved_token() {
    use agtrs::prelude::*;
    use lauren_chatbot::tools::banking_tools::TransferFundsTool;

    let state = build_test_state().await;
    let tool: Arc<TransferFundsTool> = state.container().resolve_external().await.unwrap();
    let mut tool_ctx = ToolContext::new("tu-transfer-ok");
    tool_ctx
        .extensions
        .insert(lauren_chatbot::error::UserIdExtension("alice".to_string()));
    tool_ctx.state.insert(
        "transfer_approved".to_string(),
        serde_json::json!({"to_user": "bob", "amount": 50.0, "consumed": false}),
    );

    let result = tool
        .call(
            serde_json::json!({"user_id": "alice", "to_user": "bob", "amount": 50.0}),
            &tool_ctx,
        )
        .await
        .unwrap();
    assert!(
        !result.is_error,
        "expected successful transfer, got: {}",
        result.content
    );
    assert!(
        result.content.to_lowercase().contains("success")
            || result.content.to_lowercase().contains("transfer"),
        "expected success message, got: {}",
        result.content
    );
}

// ── Knowledge base ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_knowledge_base_search_finds_savings_account_info() {
    use lauren_chatbot::knowledge::PublicKnowledgeBase;
    let kb = PublicKnowledgeBase::new();
    let results = kb.search("savings account", 3);
    assert!(
        !results.is_empty(),
        "savings account query must return results"
    );
    assert!(results.len() <= 3, "top_k respected");
}

#[tokio::test]
async fn test_knowledge_base_search_empty_for_nonsense_query() {
    use lauren_chatbot::knowledge::PublicKnowledgeBase;
    let kb = PublicKnowledgeBase::new();
    let results = kb.search("xyzzy quantum entanglement photon", 3);
    assert!(results.is_empty(), "nonsense query must return nothing");
}

#[tokio::test]
async fn test_knowledge_base_search_respects_top_k_limit() {
    use lauren_chatbot::knowledge::PublicKnowledgeBase;
    let kb = PublicKnowledgeBase::new();
    let results = kb.search("account fee branch security opening", 2);
    assert!(results.len() <= 2, "top_k=2 must be respected");
}

#[tokio::test]
async fn test_knowledge_base_has_multiple_documents() {
    use lauren_chatbot::knowledge::PublicKnowledgeBase;
    let kb = PublicKnowledgeBase::new();
    // Broad query should match multiple docs (product_catalog, fee_schedule, branch_hours, etc.)
    let results = kb.search("account fee branch security opening", 10);
    assert!(
        results.len() >= 3,
        "expected at least 3 matching docs for broad query, got {}",
        results.len()
    );
}

#[tokio::test]
async fn test_knowledge_tool_via_sse_emits_tool_result() {
    let (app, transport) = create_mock_llm_app().await;

    // LLM calls search_public_info
    transport
        .queue_tool_call(
            "search_public_info",
            serde_json::json!({"query": "savings account"}),
        )
        .await;
    transport
        .queue_text("Here's what I found about savings accounts.")
        .await;

    let resp = app
        .oneshot(post_json(
            "/api/banking/chat/public",
            serde_json::json!({"messages":[{"role":"user","content":"Tell me about savings accounts"}],"conversation_id":"kb-1"}),
        ))
        .await
        .unwrap();

    let events = collect_sse_events(resp).await;
    assert!(
        events.iter().any(|e| e["type"] == "tool_use"),
        "expected tool_execution"
    );
    // knowledge tool was invoked (tool_use event) and the stream completed (done)
    assert!(
        events.iter().any(|e| e["type"] == "done"),
        "expected done event, got: {events:?}"
    );
}

// ── Agent executor streaming (unit-level) ─────────────────────────────────────

#[tokio::test]
async fn test_agent_executor_run_stream_emits_text_delta_and_done() {
    use agtrs::agtrs_runtime::agent::{AgentConfig, AgentContext};
    use agtrs::agtrs_runtime::executor::AgentExecutor;
    use agtrs::agtrs_runtime::transport::Message;
    use agtrs::prelude::*;
    use injectable_runtime::{EmptySingletonStore, ResolveContext};

    struct SimpleAgent {
        config: AgentConfig,
    }

    #[async_trait::async_trait]
    impl Agent for SimpleAgent {
        fn name(&self) -> &str {
            "simple"
        }
        fn system_prompt(&self) -> String {
            "You are helpful.".to_string()
        }
        fn tools(&self) -> Vec<Arc<ErasedTool>> {
            vec![]
        }
        fn config(&self) -> &AgentConfig {
            &self.config
        }
    }

    let transport = Arc::new(MockTransport::new());
    transport.queue_text("Hello from mock LLM!").await;

    let llm = Arc::new(MockLlmProvider::new(Arc::clone(&transport)));
    let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
    let config = AgentConfig {
        max_turns: 3,
        ..Default::default()
    };

    let ctx = AgentContext::new("simple", config.clone(), llm, resolve_ctx);
    let agent = Arc::new(SimpleAgent { config });

    let mut stream = AgentExecutor::run_stream(agent, Message::user("hi"), ctx);

    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let has_text = events
        .iter()
        .any(|e| matches!(e, StreamEvent::TextDelta { delta } if !delta.is_empty()));
    let has_done = events.iter().any(|e| matches!(e, StreamEvent::Done { .. }));

    assert!(has_text, "expected TextDelta event, got: {events:?}");
    assert!(has_done, "expected Done event, got: {events:?}");
}

#[tokio::test]
async fn test_agent_executor_run_stream_tool_call_produces_tool_events() {
    use agtrs::agtrs_runtime::agent::{AgentConfig, AgentContext};
    use agtrs::agtrs_runtime::executor::AgentExecutor;
    use agtrs::agtrs_runtime::transport::Message;
    use agtrs::prelude::*;
    use injectable_runtime::{EmptySingletonStore, ResolveContext};

    struct EchoTool;

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        type Inputs = serde_json::Value;
        type Output = ToolResult;
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo input"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn call(
            &self,
            input: serde_json::Value,
            ctx: &ToolContext,
        ) -> Result<ToolResult, AgtrsError> {
            Ok(ToolResult::ok(input.to_string(), &ctx.tool_use_id))
        }
    }

    struct ToolAgent {
        config: AgentConfig,
    }

    #[async_trait::async_trait]
    impl Agent for ToolAgent {
        fn name(&self) -> &str {
            "tool-agent"
        }
        fn system_prompt(&self) -> String {
            "Use tools.".to_string()
        }
        fn tools(&self) -> Vec<Arc<ErasedTool>> {
            vec![Arc::new(EchoTool) as Arc<ErasedTool>]
        }
        fn config(&self) -> &AgentConfig {
            &self.config
        }
    }

    let transport = Arc::new(MockTransport::new());
    transport
        .queue_tool_call("echo", serde_json::json!({"msg": "hello"}))
        .await;
    transport.queue_text("I called the echo tool.").await;

    let llm = Arc::new(MockLlmProvider::new(Arc::clone(&transport)));
    let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
    let config = AgentConfig {
        max_turns: 3,
        ..Default::default()
    };

    let mut ctx = AgentContext::new("tool-agent", config.clone(), llm, resolve_ctx);
    ctx.register_tool("echo".to_string(), Arc::new(EchoTool) as Arc<ErasedTool>);

    let agent = Arc::new(ToolAgent { config });
    let mut stream = AgentExecutor::run_stream(agent, Message::user("echo hello"), ctx);

    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let has_tool_exec = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolExecution { .. }));
    let has_tool_result = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolResult { .. }));
    let has_done = events.iter().any(|e| matches!(e, StreamEvent::Done { .. }));

    assert!(
        has_tool_exec,
        "expected ToolExecution event, got: {events:?}"
    );
    assert!(
        has_tool_result,
        "expected ToolResult event, got: {events:?}"
    );
    assert!(has_done, "expected Done event, got: {events:?}");
}

// ── Context state propagation to tools ───────────────────────────────────────

#[tokio::test]
async fn test_agent_context_state_propagated_to_tool_context() {
    use agtrs::agtrs_runtime::agent::{AgentConfig, AgentContext};
    use agtrs::agtrs_runtime::executor::AgentExecutor;
    use agtrs::agtrs_runtime::transport::Message;
    use agtrs::prelude::*;
    use injectable_runtime::{EmptySingletonStore, ResolveContext};
    use std::collections::HashMap;

    struct CaptureStateTool {
        captured: Arc<tokio::sync::Mutex<Option<String>>>,
    }

    #[async_trait::async_trait]
    impl Tool for CaptureStateTool {
        type Inputs = serde_json::Value;
        type Output = ToolResult;
        fn name(&self) -> &str {
            "capture_state"
        }
        fn description(&self) -> &str {
            "Captures user_id from context state"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            ctx: &ToolContext,
        ) -> Result<ToolResult, AgtrsError> {
            let uid = ctx
                .state
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            *self.captured.lock().await = Some(uid.clone());
            Ok(ToolResult::ok(format!("captured: {uid}"), &ctx.tool_use_id))
        }
    }

    struct CaptureAgent {
        config: AgentConfig,
    }

    #[async_trait::async_trait]
    impl Agent for CaptureAgent {
        fn name(&self) -> &str {
            "capture-agent"
        }
        fn system_prompt(&self) -> String {
            "Capture state.".to_string()
        }
        fn tools(&self) -> Vec<Arc<ErasedTool>> {
            vec![]
        }
        fn config(&self) -> &AgentConfig {
            &self.config
        }
    }

    let captured = Arc::new(tokio::sync::Mutex::new(None::<String>));
    let cap_tool = Arc::new(CaptureStateTool {
        captured: Arc::clone(&captured),
    });

    let transport = Arc::new(MockTransport::new());
    transport
        .queue_tool_call("capture_state", serde_json::json!({}))
        .await;
    transport.queue_text("captured").await;

    let llm = Arc::new(MockLlmProvider::new(Arc::clone(&transport)));
    let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
    let config = AgentConfig {
        max_turns: 3,
        ..Default::default()
    };

    let mut ctx = AgentContext::new("capture-agent", config.clone(), llm, resolve_ctx);

    let mut state_map = HashMap::new();
    state_map.insert("user_id".to_string(), serde_json::json!("alice"));
    ctx.set_context_state(state_map);
    ctx.register_tool(
        "capture_state".to_string(),
        Arc::clone(&cap_tool) as Arc<ErasedTool>,
    );

    let agent = Arc::new(CaptureAgent { config });
    let mut stream = AgentExecutor::run_stream(agent, Message::user("capture"), ctx);
    while let Some(_) = stream.next().await {}

    let uid = captured.lock().await.clone();
    assert_eq!(
        uid.as_deref(),
        Some("alice"),
        "user_id must propagate to ToolContext.state"
    );
}

// ── SSE schemas ───────────────────────────────────────────────────────────────

#[test]
fn test_sse_event_stream_event_maps_correctly() {
    use agtrs::agtrs_runtime::streaming::StreamEvent;
    use lauren_chatbot::chat::sse::stream_event_to_sse;

    assert!(stream_event_to_sse(&StreamEvent::TextDelta { delta: "hi".into() }).is_some());
    assert!(stream_event_to_sse(&StreamEvent::ToolExecution {
        tool_name: "t".into(),
        tool_use_id: "u".into(),
    })
    .is_some());
    assert!(stream_event_to_sse(&StreamEvent::Error {
        message: "e".into()
    })
    .is_some());
    // ToolResult is suppressed (not in Python format)
    assert!(stream_event_to_sse(&StreamEvent::ToolResult {
        result: agtrs::agtrs_runtime::tool::ToolResult {
            tool_use_id: "u".into(),
            content: "c".into(),
            is_error: false,
        },
    })
    .is_none());
}

#[test]
fn test_chat_request_full_deserialization() {
    use lauren_chatbot::chat::schemas::ChatRequest;

    let full = r#"{"messages":[{"role":"user","content":"hello"}],"conversation_id":"c1","user_id":"alice"}"#;
    let req: ChatRequest = serde_json::from_str(full).unwrap();
    assert_eq!(req.last_user_message(), "hello");
    assert_eq!(req.conversation_id, Some("c1".into()));
    assert_eq!(req.user_id, Some("alice".into()));
}

#[test]
fn test_chat_request_minimal_deserialization() {
    use lauren_chatbot::chat::schemas::ChatRequest;

    let minimal = r#"{"messages":[{"role":"user","content":"hi"}]}"#;
    let req: ChatRequest = serde_json::from_str(minimal).unwrap();
    assert_eq!(req.last_user_message(), "hi");
    assert!(req.conversation_id.is_none());
    assert!(req.user_id.is_none());
}
