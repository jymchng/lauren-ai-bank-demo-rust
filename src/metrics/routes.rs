use axum::Json;
use serde_json::{json, Value};

use agtrs::agtrs_runtime;
use agtrs_runtime::cost::CostTracker;
use injectable::prelude::*;

use crate::AppState;

pub async fn summary(tracker: Inject<CostTracker>) -> Json<Value> {
    let report = tracker.report(None, None).await;
    Json(json!({
        "trace_count": report.entries.len(),
        "total_cost_usd": report.total_cost_usd,
        "total_input_tokens": report.total_input_tokens,
        "total_output_tokens": report.total_output_tokens,
    }))
}

pub async fn traces(tracker: Inject<CostTracker>) -> Json<Value> {
    let report = tracker.report(None, None).await;
    let traces: Vec<Value> = report
        .entries
        .iter()
        .rev()
        .take(50)
        .map(|entry| {
            json!({
                "model": entry.model,
                "input_tokens": entry.usage.input_tokens,
                "output_tokens": entry.usage.output_tokens,
                "cost_usd": entry.cost_usd,
                "timestamp": entry.timestamp.to_rfc3339(),
            })
        })
        .collect();
    Json(json!({"traces": traces}))
}

pub async fn cost(tracker: Inject<CostTracker>) -> Json<Value> {
    let report = tracker.report(None, None).await;
    let mut by_model: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for entry in &report.entries {
        *by_model.entry(entry.model.clone()).or_default() += entry.cost_usd;
    }
    Json(json!({
        "by_model": by_model,
        "total_cost_usd": report.total_cost_usd,
    }))
}

pub fn metrics_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/metrics", axum::routing::get(summary))
        .route("/api/metrics/traces", axum::routing::get(traces))
        .route("/api/metrics/cost", axum::routing::get(cost))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{create_test_app, create_test_state};
    use agtrs::agtrs_runtime::transport::TokenUsage;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_metrics_summary() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_traces() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics/traces")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_cost() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics/cost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let state = create_test_state().await;
        let tracker: Arc<CostTracker> = state.container().resolve_external().await.unwrap();
        let report = tracker.report(None, None).await;
        assert_eq!(report.entries.len(), 0);
        assert_eq!(report.total_cost_usd, 0.0);
    }

    #[tokio::test]
    async fn test_get_traces() {
        let state = create_test_state().await;
        let tracker: Arc<CostTracker> = state.container().resolve_external().await.unwrap();
        let report = tracker.report(None, None).await;
        assert!(report.entries.is_empty());
    }

    #[tokio::test]
    async fn test_get_cost() {
        let state = create_test_state().await;
        let tracker: Arc<CostTracker> = state.container().resolve_external().await.unwrap();
        let report = tracker.report(None, None).await;
        assert_eq!(report.total_cost_usd, 0.0);
    }

    #[tokio::test]
    async fn test_metrics_with_usage() {
        let state = create_test_state().await;
        let tracker: Arc<CostTracker> = state.container().resolve_external().await.unwrap();
        tracker
            .record_usage("gpt-4", &TokenUsage::new(100, 50), "conv1")
            .await;
        // Verify via HTTP using the same state (same CostTracker singleton)
        let app = crate::create_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
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
        assert!(data.get("trace_count").unwrap().as_u64().unwrap() > 0);
    }
}
