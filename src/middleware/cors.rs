use tower_http::cors::{Any, CorsLayer};

/// Create a CORS layer that allows all origins.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

mod tests {
    use super::*;

    #[test]
    fn test_cors_layer_creation() {
        let _layer = cors_layer();
        // Just verify it creates without panicking
    }
}
