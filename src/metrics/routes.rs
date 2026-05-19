use agtrs::agtrs_runtime;
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

/// Metrics summary endpoint.
pub async fn summary(State(state): State<Arc<AppState>>) -> Json<Value> {
    let report = state.cost_tracker.report(None, None).await;
    Json(json!({
        "trace_count": report.entries.len(),
        "total_cost_usd": report.total_cost_usd,
        "total_input_tokens": report.total_input_tokens,
        "total_output_tokens": report.total_output_tokens,
    }))
}

/// Traces endpoint.
pub async fn traces(State(state): State<Arc<AppState>>) -> Json<Value> {
    let report = state.cost_tracker.report(None, None).await;
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

/// Cost breakdown endpoint.
pub async fn cost(State(state): State<Arc<AppState>>) -> Json<Value> {
    let report = state.cost_tracker.report(None, None).await;

    // Group by model
    let mut by_model: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for entry in &report.entries {
        *by_model.entry(entry.model.clone()).or_default() += entry.cost_usd;
    }

    Json(json!({
        "by_model": by_model,
        "total_cost_usd": report.total_cost_usd,
    }))
}

/// Create the metrics router.
pub fn metrics_router() -> axum::Router<Arc<AppState>> {
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
    async fn test_metrics_summary_direct() {
        let state = create_test_state().await;
        let result = summary(axum::extract::State(state)).await;
        let data = result.0;
        assert_eq!(data.get("trace_count").unwrap().as_u64().unwrap(), 0);
        assert_eq!(data.get("total_cost_usd").unwrap().as_f64().unwrap(), 0.0);
    }

    #[tokio::test]
    async fn test_metrics_traces_direct() {
        let state = create_test_state().await;
        let result = traces(axum::extract::State(state)).await;
        let data = result.0;
        let traces = data.get("traces").unwrap().as_array().unwrap();
        assert!(traces.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_cost_direct() {
        let state = create_test_state().await;
        let result = cost(axum::extract::State(state)).await;
        let data = result.0;
        assert!(data.get("by_model").unwrap().is_object());
        assert_eq!(data.get("total_cost_usd").unwrap().as_f64().unwrap(), 0.0);
    }

    #[tokio::test]
    async fn test_metrics_with_usage() {
        let state = create_test_state().await;
        state
            .cost_tracker
            .record_usage("gpt-4", &TokenUsage::new(100, 50), "conv1")
            .await;
        let result = summary(axum::extract::State(state)).await;
        let data = result.0;
        assert!(data.get("trace_count").unwrap().as_u64().unwrap() > 0);
    }
}
