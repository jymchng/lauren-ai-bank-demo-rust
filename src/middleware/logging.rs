use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;
use tracing::info;

/// Middleware that adds X-Response-Time header.
pub async fn timing_middleware(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_millis();
    let mut response = response;
    response
        .headers_mut()
        .insert("X-Response-Time", format!("{}ms", elapsed).parse().unwrap());
    response
}

/// Middleware that logs request method, path, and status.
pub async fn logging_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().clone();
    let response = next.run(request).await;
    info!(
        %method, %path, status = response.status().as_u16(),
        "request completed"
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use tower::ServiceExt;

    async fn test_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_timing_middleware_adds_header() {
        let app = Router::new()
            .route("/test", axum::routing::get(test_handler))
            .layer(axum::middleware::from_fn(timing_middleware));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let header = response.headers().get("X-Response-Time").unwrap();
        assert!(header.to_str().unwrap().ends_with("ms"));
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let app = Router::new()
            .route("/test", axum::routing::get(test_handler))
            .layer(axum::middleware::from_fn(logging_middleware));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
