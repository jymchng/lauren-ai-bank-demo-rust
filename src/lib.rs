pub mod agents;
pub mod approval;
pub mod banking;
pub mod chat;
pub mod config;
pub mod crypto;
pub mod error;
pub mod guardrails;
pub mod health;
pub mod knowledge;
pub mod llm;
pub mod metrics;
pub mod middleware;
pub mod signals;
pub mod tools;
pub mod ws;

use std::sync::Arc;

use agtrs::prelude::*;
use agtrs_runtime::cost::{CostTracker, PricingTable};
use agtrs_runtime::memory::InMemoryConversationStore;
use agtrs_runtime::team::HandoffAgentStore;
use injectable::axum::{AxumState, InjectableState};
use injectable::prelude::*;
use injectable_runtime::ResolveContext;
use tower_http::trace::TraceLayer;

// Bind the concrete LlmProvider implementation (compile-time inventory entry).
// Must be in the same compilation unit as the #[injectable] types it references.

/// Minimal application state — only holds things that cannot be resolved via
/// `Inject<T>` (non-`#[injectable]` trait objects and the DI container itself).
///
/// Every `#[injectable]` service (BankDatabase, ApprovalService, agents, …)
/// is extracted directly in handlers via `Inject<T>`.  `CostTracker` and
/// `InMemoryConversationStore` are registered as `DynProvider<Arc<T>>`
/// singletons so they too are available via `Inject<T>`.
#[derive(Clone)]
pub struct AppState {
    /// Wraps `Arc<Container>` — the source of all DI resolutions.
    container: AxumState,
    /// LLM provider — `Arc<dyn LlmProvider>` is not Sized, so `Inject<dyn LlmProvider>`
    /// cannot be used as a handler extractor; stored here instead.
    pub llm: Arc<dyn LlmProvider>,
}

impl InjectableState for AppState {
    fn resolve_context(&self) -> &ResolveContext {
        self.container.resolve_context()
    }
}

impl AppState {
    /// Borrow the inner `Container` for direct resolution in tests or startup code.
    pub fn container(&self) -> &injectable::Container {
        self.container.container()
    }

    /// Return a clone with a different LLM provider (used in tests to inject a mock).
    pub fn with_llm(self, llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm, ..self }
    }
}

/// Build the application state using the injectable container.
pub async fn build_app_state() -> AppState {
    // Pre-build singletons that aren't `#[injectable]` and register them as
    // `DynProvider<Arc<T>>` so `Inject<T>` can extract them in handlers.
    let conv_store = Arc::new(InMemoryConversationStore::new());
    let cost_tracker = Arc::new(CostTracker::new(Arc::new(PricingTable::default_pricing())));
    let agent_store = Arc::new(HandoffAgentStore::new());

    let conv_store_dyn = Arc::clone(&conv_store);
    let cost_tracker_dyn = Arc::clone(&cost_tracker);
    let agent_store_dyn = Arc::clone(&agent_store);

    let container = Container::builder()
        .register(
            "",
            DynProvider::<Arc<InMemoryConversationStore>>::sync(move || {
                Ok(Arc::clone(&conv_store_dyn))
            }),
        )
        .register(
            "",
            DynProvider::<Arc<CostTracker>>::sync(move || Ok(Arc::clone(&cost_tracker_dyn))),
        )
        .register(
            "",
            DynProvider::<Arc<HandoffAgentStore>>::sync(move || Ok(Arc::clone(&agent_store_dyn))),
        )
        .build()
        .await
        .expect("DI container failed to build — check injectable bindings");

    let llm = container
        .resolve_external::<Arc<dyn LlmProvider>>()
        .await
        .expect("Failed to resolve LlmProvider");

    AppState {
        container: AxumState::new(container),
        llm,
    }
}

/// Build state for tests — uses a real container but with test config defaults.
pub async fn build_test_state() -> AppState {
    std::env::set_var("OPENROUTER_API_KEY", "test-key");
    std::env::set_var("LLM_MODEL", "test-model");
    std::env::set_var("LLM_BASE_URL", "http://localhost:11434/v1");
    std::env::set_var("PAYLOAD_SECRET", "test-secret-key-for-hmac");
    std::env::set_var("PORT", "8000");
    build_app_state().await
}

/// Create the complete Axum router with all routes and middleware.
pub fn create_router(state: AppState) -> axum::Router {
    let app = axum::Router::new()
        .merge(health::routes::health_router())
        .merge(banking::routes::banking_router())
        .merge(chat::chat_router())
        .merge(approval::routes::approval_router())
        .merge(ws::gateway::ws_router())
        .merge(metrics::routes::metrics_router())
        .with_state(state);

    let app = app.layer(axum::middleware::from_fn(
        middleware::logging::timing_middleware,
    ));
    app.layer(middleware::cors::cors_layer())
        .layer(TraceLayer::new_for_http())
}

/// Test utilities module.
pub mod test_utils {
    use super::*;

    pub async fn create_test_state() -> AppState {
        build_test_state().await
    }

    pub async fn create_test_app() -> axum::Router {
        let state = create_test_state().await;
        create_router(state)
    }
}

mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[tokio::test]
    async fn test_build_app_state() {
        let state = test_utils::create_test_state().await;
        let config: Arc<AppConfig> = state
            .container()
            .resolve_external()
            .await
            .expect("AppConfig");
        assert_eq!(config.port, 8000);
    }

    #[tokio::test]
    async fn test_app_state_clone() {
        let state = test_utils::create_test_state().await;
        let cloned = state.clone();
        let p1: Arc<AppConfig> = state.container().resolve_external().await.unwrap();
        let p2: Arc<AppConfig> = cloned.container().resolve_external().await.unwrap();
        assert_eq!(p1.port, p2.port);
    }
}
