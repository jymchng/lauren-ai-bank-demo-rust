use agtrs::agtrs_runtime;
//! Multi-agent orchestration loop.
//!
//! Manages handoffs between agents within a single conversation.
//! Agents and services are resolved from the injectable DI container,
//! making tool dependencies (BankDatabase, ApprovalService, etc.) available
//! to tools at call time via `ctx.resolve_context()`.

use std::sync::Arc;

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::agents::active_agent_store::ActiveAgentStore;
use crate::agents::auth_crm::AuthenticatedCrmAgent;
use crate::agents::disputes::DisputesAgent;
use crate::agents::transfer::BankTransferAgent;
use crate::agents::unauth_crm::UnauthenticatedCrmAgent;
use crate::error::AppError;

/// Maximum handoffs for authenticated users.
const MAX_AUTH_HANDOFFS: usize = 8;
/// Maximum handoffs for public (unauthenticated) users.
const MAX_PUBLIC_HANDOFFS: usize = 4;

/// The multi-agent orchestrator.
///
/// Holds pre-resolved agent singletons and the shared ResolveContext so that
/// tools can access DI services (BankDatabase, ApprovalService, etc.) at call time.
pub struct Orchestrator {
    unauth_crm: Arc<UnauthenticatedCrmAgent>,
    auth_crm: Arc<AuthenticatedCrmAgent>,
    transfer_agent: Arc<BankTransferAgent>,
    disputes_agent: Arc<DisputesAgent>,
    active_agent_store: Arc<ActiveAgentStore>,
    llm: Arc<dyn LlmProvider>,
    /// Shared container ResolveContext — passed to each AgentContext so tools
    /// can call `ctx.resolve_context().resolve_external::<Arc<Service>>()`.
    resolve_ctx: Arc<ResolveContext>,
}

impl Orchestrator {
    /// Create an orchestrator from the injectable container's resolved singletons.
    pub fn new(
        unauth_crm: Arc<UnauthenticatedCrmAgent>,
        auth_crm: Arc<AuthenticatedCrmAgent>,
        transfer_agent: Arc<BankTransferAgent>,
        disputes_agent: Arc<DisputesAgent>,
        active_agent_store: Arc<ActiveAgentStore>,
        llm: Arc<dyn LlmProvider>,
        resolve_ctx: Arc<ResolveContext>,
    ) -> Self {
        Self { unauth_crm, auth_crm, transfer_agent, disputes_agent, active_agent_store, llm, resolve_ctx }
    }

    /// Run the orchestration loop for an authenticated user.
    pub async fn run_authenticated(
        &self,
        message: &str,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<AgentResponse, AppError> {
        self.run_inner(message, conversation_id, Some(user_id), MAX_AUTH_HANDOFFS).await
    }

    /// Run the orchestration loop for a public (unauthenticated) user.
    pub async fn run_public(
        &self,
        message: &str,
        conversation_id: &str,
    ) -> Result<AgentResponse, AppError> {
        self.run_inner(message, conversation_id, None, MAX_PUBLIC_HANDOFFS).await
    }

    async fn run_inner(
        &self,
        message: &str,
        conversation_id: &str,
        user_id: Option<&str>,
        max_handoffs: usize,
    ) -> Result<AgentResponse, AppError> {
        let mut current_agent_name = self.active_agent_store
            .get_active_agent(conversation_id).await
            .unwrap_or_else(|| {
                if user_id.is_some() { "authenticated_crm".into() } else { "unauthenticated_crm".into() }
            });

        let mut cumulative_response = String::new();
        let mut total_turns = 0;
        let mut total_usage = TokenUsage::default();

        for handoff in 0..max_handoffs {
            let agent = self.get_agent(&current_agent_name)
                .ok_or_else(|| AppError::Internal(format!("Unknown agent: {current_agent_name}")))?;

            let pending_summary = self.active_agent_store.get_pending_summary(conversation_id).await;
            self.active_agent_store.clear_pending_summary(conversation_id).await;

            let mut ctx = AgentContext::new(
                &current_agent_name,
                agent.config().clone(),
                Arc::clone(&self.llm),
                Arc::clone(&self.resolve_ctx),  // ← real container context, not empty
            );

            // Inject runtime state into ToolContext via agent metadata
            ctx.metadata_mut().insert("conversation_id".into(), conversation_id.into());
            if let Some(uid) = user_id {
                ctx.metadata_mut().insert("user_id".into(), uid.into());
            }

            for tool in agent.tools() {
                let name = tool.name().to_string();
                ctx.register_tool(name, tool);
            }

            let input = if let Some(summary) = pending_summary {
                if handoff == 0 {
                    Message::user(message)
                } else {
                    Message::user(format!(
                        "[Context from previous agent]: {summary}\n\n[User's original message]: {message}"
                    ))
                }
            } else {
                Message::user(message)
            };

            let response = agent.run(input, &mut ctx).await?;

            cumulative_response = response.content.clone();
            total_turns += response.turns;
            total_usage.add(&response.total_usage);

            let new_agent = self.active_agent_store.get_active_agent(conversation_id).await;
            match new_agent {
                Some(name) if name != current_agent_name => {
                    current_agent_name = name;
                    continue;
                }
                _ => {
                    return Ok(AgentResponse {
                        content: cumulative_response,
                        turns: total_turns,
                        total_usage,
                        tool_calls_made: vec![],
                        stop_reason: response.stop_reason,
                        metadata: response.metadata,
                        reasoning_traces: response.reasoning_traces,
                    });
                }
            }
        }

        Err(AppError::AgentError(format!("Maximum handoffs ({max_handoffs}) exceeded")))
    }

    fn get_agent(&self, name: &str) -> Option<Arc<dyn Agent>> {
        match name {
            "unauthenticated_crm" => Some(Arc::clone(&self.unauth_crm) as Arc<dyn Agent>),
            "authenticated_crm"   => Some(Arc::clone(&self.auth_crm) as Arc<dyn Agent>),
            "bank_transfer"       => Some(Arc::clone(&self.transfer_agent) as Arc<dyn Agent>),
            "disputes"            => Some(Arc::clone(&self.disputes_agent) as Arc<dyn Agent>),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use injectable::prelude::*;

    async fn create_orchestrator() -> Orchestrator {
        let container = Container::builder().build().await.unwrap();
        let resolve_ctx = Arc::new(container.context().clone());

        macro_rules! get {
            ($T:ty) => { container.resolve_external::<Arc<$T>>().await.unwrap() }
        }

        let transport = Arc::new(agtrs_runtime::testing::MockTransport::new());
        let llm: Arc<dyn LlmProvider> = Arc::new(agtrs_runtime::testing::MockLlmProvider::new(transport));

        Orchestrator::new(
            get!(UnauthenticatedCrmAgent),
            get!(AuthenticatedCrmAgent),
            get!(BankTransferAgent),
            get!(DisputesAgent),
            get!(ActiveAgentStore),
            llm,
            resolve_ctx,
        )
    }

    #[tokio::test]
    async fn test_get_agent() {
        let orchestrator = create_orchestrator().await;
        assert!(orchestrator.get_agent("unauthenticated_crm").is_some());
        assert!(orchestrator.get_agent("authenticated_crm").is_some());
        assert!(orchestrator.get_agent("bank_transfer").is_some());
        assert!(orchestrator.get_agent("disputes").is_some());
        assert!(orchestrator.get_agent("unknown").is_none());
    }
}
