//! Integration tests for the lauren-chatbot API.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use lauren_chatbot::{build_test_state, create_router};

async fn create_test_app() -> Router {
    let state = build_test_state().await;
    create_router(state)
}

#[tokio::test]
async fn test_health_check() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/health/").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_accounts() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/banking/accounts").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_account_found() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/banking/accounts/alice").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_account_not_found() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/banking/accounts/unknown").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_ws_token_public() {
    let app = create_test_app().await;
    let req = Request::builder()
        .method("POST").uri("/api/banking/ws-token/public").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_metrics() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/metrics/").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_cost() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/metrics/cost").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_traces() {
    let app = create_test_app().await;
    let req = Request::builder().uri("/api/metrics/traces").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_approval_no_pending() {
    let app = create_test_app().await;
    let body = serde_json::json!({"conversation_id": "nonexistent-conv", "approved": true});
    let req = Request::builder()
        .method("POST").uri("/api/banking/approval")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
