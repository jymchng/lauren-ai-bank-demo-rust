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
use agtrs_runtime::memory::InMemoryConversationStore;
use injectable::prelude::*;

use agents::active_agent_store::ActiveAgentStore;
use agents::auth_crm::AuthenticatedCrmAgent;
use agents::disputes::DisputesAgent;
use agents::transfer::BankTransferAgent;
use agents::unauth_crm::UnauthenticatedCrmAgent;
use approval::service::ApprovalService;
use banking::db::BankDatabase;
use config::AppConfig;
use crypto::service::CryptoService;
use signals::bus::AppSignalBus;
use tower_http::trace::TraceLayer;
use ws::event_forwarder::EventForwarder;
use ws::token_service::WsTokenService;

// Bind the concrete LlmProvider implementation (compile-time inventory entry).
// Must be in the same compilation unit as the #[injectable] types it references.
use llm::OpenRouterProvider;

/// Shared application state accessible to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub bank_db: Arc<BankDatabase>,
    pub crypto_service: Arc<CryptoService>,
    pub active_agent_store: Arc<ActiveAgentStore>,
    pub approval_service: Arc<ApprovalService>,
    pub signal_bus: Arc<AppSignalBus>,
    pub cost_tracker: Arc<CostTracker>,
    pub event_forwarder: Arc<EventForwarder>,
    pub ws_token_service: Arc<WsTokenService>,
    /// Pre-resolved agents — singletons from the DI container.
    pub unauth_agent: Arc<UnauthenticatedCrmAgent>,
    pub auth_agent: Arc<AuthenticatedCrmAgent>,
    pub transfer_agent: Arc<BankTransferAgent>,
    pub disputes_agent: Arc<DisputesAgent>,
    /// Shared ResolveContext — passed to AgentContext so tools can resolve services.
    pub resolve_ctx: Arc<injectable_runtime::ResolveContext>,
    /// The LLM provider — passed to AgentContext for streaming calls.
    pub llm: Arc<dyn LlmProvider>,
    /// Shared conversation store — persists history across HTTP requests.
    pub conv_store: Arc<InMemoryConversationStore>,
}

impl AppState {
    /// Get an agent arc by logical name.
    pub fn get_agent_by_name(&self, name: &str) -> Option<Arc<dyn Agent>> {
        match name {
            "Banking CRM Agent (Public)" => Some(Arc::clone(&self.unauth_agent) as Arc<dyn Agent>),
            "Banking CRM Agent (Authenticated)" => {
                Some(Arc::clone(&self.auth_agent) as Arc<dyn Agent>)
            }
            "Banking Transfer Agent" => Some(Arc::clone(&self.transfer_agent) as Arc<dyn Agent>),
            "Banking Disputes Agent" => Some(Arc::clone(&self.disputes_agent) as Arc<dyn Agent>),
            _ => None,
        }
    }
}

/// Build the application state using the injectable container.
pub async fn build_app_state() -> Arc<AppState> {
    let container = Container::builder()
        .build()
        .await
        .expect("DI container failed to build — check injectable bindings");

    let resolve_ctx = Arc::new(container.context().clone());

    macro_rules! get {
        ($T:ty) => {
            container
                .resolve_external::<Arc<$T>>()
                .await
                .unwrap_or_else(|e| panic!("Failed to resolve {}: {e}", stringify!($T)))
        };
    }

    let llm = container
        .resolve_external::<Arc<dyn LlmProvider>>()
        .await
        .unwrap_or_else(|e| panic!("Failed to resolve LlmProvider: {e}"));

    Arc::new(AppState {
        config: get!(AppConfig),
        bank_db: get!(BankDatabase),
        crypto_service: get!(CryptoService),
        active_agent_store: get!(ActiveAgentStore),
        approval_service: get!(ApprovalService),
        signal_bus: get!(AppSignalBus),
        event_forwarder: get!(EventForwarder),
        ws_token_service: get!(WsTokenService),
        cost_tracker: Arc::new(CostTracker::new(Arc::new(PricingTable::default_pricing()))),
        unauth_agent: get!(UnauthenticatedCrmAgent),
        auth_agent: get!(AuthenticatedCrmAgent),
        transfer_agent: get!(BankTransferAgent),
        disputes_agent: get!(DisputesAgent),
        resolve_ctx,
        llm,
        conv_store: Arc::new(InMemoryConversationStore::new()),
    })
}

/// Build state for tests — uses a real container but with test config defaults.
pub async fn build_test_state() -> Arc<AppState> {
    // Override env vars for test config before building container
    std::env::set_var("OPENROUTER_API_KEY", "test-key");
    std::env::set_var("LLM_MODEL", "test-model");
    std::env::set_var("LLM_BASE_URL", "http://localhost:11434/v1");
    std::env::set_var("PAYLOAD_SECRET", "test-secret-key-for-hmac");
    std::env::set_var("PORT", "8000");
    build_app_state().await
}

/// Create the complete Axum router with all routes and middleware.
pub fn create_router(state: Arc<AppState>) -> axum::Router {
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

    /// Create a test application state via the injectable container.
    pub async fn create_test_state() -> Arc<AppState> {
        build_test_state().await
    }

    /// Create a test Axum application.
    pub async fn create_test_app() -> axum::Router {
        let state = create_test_state().await;
        create_router(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_app_state() {
        let state = test_utils::create_test_state().await;
        assert_eq!(state.config.port, 8000);
    }

    #[tokio::test]
    async fn test_app_state_clone() {
        let state = test_utils::create_test_state().await;
        let cloned = state.clone();
        assert_eq!(cloned.config.port, state.config.port);
    }
}
