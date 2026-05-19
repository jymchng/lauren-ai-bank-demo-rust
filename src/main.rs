use lauren_chatbot::{build_app_state, create_router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Build application state via injectable container.
    // Config is loaded from environment variables inside the container.
    let state = build_app_state().await;
    let port = state.config.port;

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("Server running on port {port}");
    axum::serve(listener, app).await?;

    Ok(())
}
