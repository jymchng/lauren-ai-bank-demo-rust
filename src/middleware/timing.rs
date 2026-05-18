//! Response timing middleware.

use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

/// Middleware that adds `X-Response-Time` header.
pub async fn timing_middleware(request: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let mut response = next.run(request).await;
    let elapsed = start.elapsed();

    let headers = response.headers_mut();
    headers.insert(
        "X-Response-Time",
        format!("{:.2}ms", elapsed.as_secs_f64() * 1000.0)
            .parse()
            .unwrap_or_else(|_| "0ms".parse().unwrap()),
    );

    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::timing_middleware;

    async fn hello_handler() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_timing_header() {
        let app = Router::new()
            .route("/", get(hello_handler))
            .layer(middleware::from_fn(timing_middleware));

        let req = Request::builder()
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key("X-Response-Time"));
    }
}
