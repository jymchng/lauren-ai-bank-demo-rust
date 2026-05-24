use agtrs::agtrs_runtime;
//! LLM-based scope guardrail.
//!
//! Uses a secondary LLM call as a judge to verify that agent responses
//! stay within their defined scope.

use async_trait::async_trait;
use agtrs::agtrs_runtime::guardrail::{Guardrail, GuardrailContext, GuardrailDecision};
use agtrs::agtrs_runtime::transport::Message;
use agtrs::agtrs_runtime::llm::LlmResponse;
use agtrs::agtrs_runtime::tool::ToolResult;

/// Per-agent scope descriptions used by the LLM scope guard.
pub const SCOPE_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "Banking CRM Agent (Authenticated)",
        "Answering account balance inquiries, transaction history, and general account questions. \
         NOT: initiating transfers, handling disputes, or performing account modifications.",
    ),
    (
        "Banking Transfer Agent",
        "Initiating and executing fund transfers between accounts with proper approval. \
         NOT: handling disputes or general account inquiries beyond transfer context.",
    ),
    (
        "Banking Disputes Agent",
        "Investigating and resolving transaction disputes and chargebacks. \
         NOT: initiating transfers or providing general banking advice beyond disputes.",
    ),;

/// An output guardrail that uses keyword matching to check if responses
/// stay within the agent's scope. In production, this would use an LLM call
/// as a judge. For simplicity, we use keyword-based heuristics.
pub struct LlmScopeGuard {
    /// Keywords that indicate out-of-scope responses per agent.
    out_of_scope_keywords: Vec<String>,
}

impl LlmScopeGuard {
    /// Create a new LLM scope guard for a specific agent.
    pub fn for_agent(agent_name: &str) -> Self {
        let keywords = match agent_name {
            "Banking CRM Agent (Authenticated)" => vec![
                "transfer funds".into(),
                "initiate transfer".into(),
                "dispute a charge".into(),
                "file a dispute".into(),
            ],
            "Banking Transfer Agent" => vec![
                "dispute a charge".into(),
                "file a dispute".into(),
                "investment advice".into(),
            ],
            "Banking Disputes Agent" => vec![
                "transfer funds".into(),
                "initiate transfer".into(),
                "investment advice".into(),
            ],
            _ => vec![],
        };
        Self {
            out_of_scope_keywords: keywords,
        }
    }

    /// Create a generic scope guard.
    pub fn new() -> Self {
        Self {
            out_of_scope_keywords: vec![],
        }
    }
}

impl Default for LlmScopeGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Guardrail for LlmScopeGuard {
    fn name(&self) -> &str {
        "llm_scope_guard"
    }

    async fn check_input(
        &self,
        _message: &Message,
        _context: &GuardrailContext,
    ) -> GuardrailDecision {
        GuardrailDecision::Pass
    }

    async fn check_output(
        &self,
        response: &LlmResponse,
        context: &GuardrailContext,
    ) -> GuardrailDecision {
        let text = response.text().to_lowercase();

        for keyword in &self.out_of_scope_keywords {
            if text.contains(&keyword.to_lowercase()) {
                return GuardrailDecision::Block(format!(
                    "Response contains out-of-scope content for agent '{}': '{}'",
                    context.agent_name, keyword
                ));
            }
        }

        GuardrailDecision::Pass
    }

    async fn check_tool_call(
        &self,
        _tool_name: &str,
        _input: &serde_json::Value,
        _context: &GuardrailContext,
    ) -> GuardrailDecision {
        GuardrailDecision::Pass
    }

    async fn check_tool_result(
        &self,
        _result: &ToolResult,
        _context: &GuardrailContext,
    ) -> GuardrailDecision {
        GuardrailDecision::Pass
    }

    fn priority(&self) -> u32 {
        50
    }
}

mod tests {
    use super::*;
    use agtrs::agtrs_runtime::transport::TokenUsage;

    fn make_response(text: &str) -> LlmResponse {
        LlmResponse {
            message: Message::assistant(text),
            usage: TokenUsage::default(),
            tool_calls: vec![],
            finish_reason: agtrs_runtime::transport::StopReason::EndTurn,
            thinking_blocks: vec![],
        }
    }

    #[tokio::test]
    async fn test_scope_guard_passes_in_scope() {
        let guard = LlmScopeGuard::for_agent("Banking CRM Agent (Authenticated)");
        let response = make_response("Your balance is $5,000.");
        let ctx = GuardrailContext::new("Banking CRM Agent (Authenticated)");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_scope_guard_blocks_out_of_scope() {
        let guard = LlmScopeGuard::for_agent("Banking CRM Agent (Authenticated)");
        let response = make_response("I can help you transfer funds to another account.");
        let ctx = GuardrailContext::new("Banking CRM Agent (Authenticated)");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_scope_guard_default_passes() {
        let guard = LlmScopeGuard::new();
        let response = make_response("Anything goes.");
        let ctx = GuardrailContext::new("unknown");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_scope_guard_disputes() {
        let guard = LlmScopeGuard::for_agent("Banking Disputes Agent");
        let response = make_response("I'll help you transfer funds now.");
        let ctx = GuardrailContext::new("Banking Disputes Agent");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_scope_guard_input_always_passes() {
        let guard = LlmScopeGuard::for_agent("Banking CRM Agent (Authenticated)");
        let message = Message::user("transfer funds please");
        let ctx = GuardrailContext::new("Banking CRM Agent (Authenticated)");
        let decision = guard.check_input(&message, &ctx).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_priority() {
        let guard = LlmScopeGuard::new();
        assert_eq!(guard.priority(), 50);
    }
}
