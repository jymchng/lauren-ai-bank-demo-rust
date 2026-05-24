use agtrs::agtrs_runtime;
use agtrs::agtrs_runtime::guardrail::{Guardrail, GuardrailContext, GuardrailDecision};
use agtrs::agtrs_runtime::llm::LlmResponse;
use agtrs::agtrs_runtime::tool::ToolResult;
use agtrs::agtrs_runtime::transport::Message;
use async_trait::async_trait;

/// LLM-as-judge output guardrail that evaluates scope compliance.
pub struct LlmScopeGuard;

impl LlmScopeGuard {
    /// Create a new LlmScopeGuard.
    pub fn new() -> Self {
        Self
    }

    /// Get the scope description for a given agent.
    pub fn scope_for_agent(agent_name: &str) -> &'static str {
        match agent_name {
            "AuthenticatedCRM" => "Authenticated CRM Agent — can use banking tools, initiate transfers/disputes, check auth. Redirects to public assistant for product info.",
            "BankTransfer" => "Transfer Agent — only gather transfer details, request approval, execute transfer. Redirects to CRM for anything else.",
            "Disputes" => "Disputes Agent — only review transactions, gather dispute info, handoff. Redirects to CRM for anything else.",
            _ => "",
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
        let scope = Self::scope_for_agent(&context.agent_name);
        if scope.is_empty() {
            return GuardrailDecision::Pass;
        }

        let response_text = response.text();

        // Simple keyword-based check as a lightweight alternative to LLM-as-judge
        // In production, this would make a secondary LLM call
        let out_of_scope_indicators = [
            "hack",
            "exploit",
            "bypass security",
            "steal",
            "illegal",
            "money laundering",
            "fraud scheme",
        ];

        let lower = response_text.to_lowercase();
        for indicator in &out_of_scope_indicators {
            if lower.contains(indicator) {
                return GuardrailDecision::Modify(Message::assistant(
                    "I'm not able to help with that request. Let me redirect you to the appropriate assistant.",
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
}

/// Keyword-based output guardrail.
pub struct AgentScopeGuard {
    off_topic_phrases: Vec<String>,
}

impl AgentScopeGuard {
    /// Create a new AgentScopeGuard with the given off-topic phrases.
    pub fn new(phrases: Vec<String>) -> Self {
        Self {
            off_topic_phrases: phrases,
        }
    }

    /// Create a guard with common banking off-topic phrases.
    pub fn banking_default() -> Self {
        Self::new(vec![
            "hack".into(),
            "exploit".into(),
            "bypass security".into(),
            "steal money".into(),
        ])
    }
}

#[async_trait]
impl Guardrail for AgentScopeGuard {
    fn name(&self) -> &str {
        "agent_scope_guard"
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
        _context: &GuardrailContext,
    ) -> GuardrailDecision {
        let text = response.text().to_lowercase();
        for phrase in &self.off_topic_phrases {
            if text.contains(&phrase.to_lowercase()) {
                return GuardrailDecision::Block(format!(
                    "Response contains off-topic content: '{}'",
                    phrase
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
}

mod tests {
    use super::*;
    use agtrs::agtrs_runtime::transport::{Message, StopReason, TokenUsage};

    fn make_response(text: &str) -> LlmResponse {
        LlmResponse {
            message: Message::assistant(text),
            usage: TokenUsage::new(10, 20),
            tool_calls: vec![],
            finish_reason: StopReason::EndTurn,
            thinking_blocks: vec![],
        }
    }

    fn make_context(agent_name: &str) -> GuardrailContext {
        GuardrailContext::new(agent_name)
    }

    #[tokio::test]
    async fn test_llm_scope_guard_pass() {
        let guard = LlmScopeGuard::new();
        let response = make_response("Your balance is $5000.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_block_hack() {
        let guard = LlmScopeGuard::new();
        let response = make_response("I can help you hack into a bank account.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_modify());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_unknown_agent() {
        let guard = LlmScopeGuard::new();
        let response = make_response("Anything goes.");
        let context = make_context("UnknownAgent");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_input_always_passes() {
        let guard = LlmScopeGuard::new();
        let message = Message::user("hack the bank");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_input(&message, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_default() {
        let _guard = LlmScopeGuard::default();
    }

    #[tokio::test]
    async fn test_agent_scope_guard_pass() {
        let guard = AgentScopeGuard::banking_default();
        let response = make_response("Your balance is $5000.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_agent_scope_guard_block() {
        let guard = AgentScopeGuard::banking_default();
        let response = make_response("I can help you hack into the system.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_agent_scope_guard_custom_phrases() {
        let guard = AgentScopeGuard::new(vec!["crypto".into(), "bitcoin".into()]);
        let response = make_response("You should invest in bitcoin.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_block());
    }

    #[test]
    fn test_scope_for_agent() {
        assert!(!LlmScopeGuard::scope_for_agent("AuthenticatedCRM").is_empty());
        assert!(!LlmScopeGuard::scope_for_agent("BankTransfer").is_empty());
        assert!(!LlmScopeGuard::scope_for_agent("Disputes").is_empty());
        assert!(LlmScopeGuard::scope_for_agent("Unknown").is_empty());
    }

    #[tokio::test]
    async fn test_guard_tool_call_always_passes() {
        let guard = LlmScopeGuard::new();
        let context = make_context("AuthenticatedCRM");
        let decision = guard
            .check_tool_call("get_balance", &serde_json::json!({}), &context)
            .await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_guard_tool_result_always_passes() {
        let guard = LlmScopeGuard::new();
        let context = make_context("AuthenticatedCRM");
        let result = ToolResult::ok("Balance: $5000", "tool_1");
        let decision = guard.check_tool_result(&result, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_steal_keyword() {
        let guard = LlmScopeGuard::new();
        let response = make_response("I can help you steal money from the bank.");
        let context = make_context("BankTransfer");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_modify());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_exploit_keyword() {
        let guard = LlmScopeGuard::new();
        let response = make_response("There is an exploit in the system.");
        let context = make_context("Disputes");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_modify());
    }

    #[tokio::test]
    async fn test_llm_scope_guard_money_laundering_keyword() {
        let guard = LlmScopeGuard::new();
        let response = make_response("I can help with money laundering.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_modify());
    }

    #[tokio::test]
    async fn test_agent_scope_guard_no_match() {
        let guard = AgentScopeGuard::banking_default();
        let response = make_response("Your account is secure and well-protected.");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_output(&response, &context).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_agent_scope_guard_input_passes() {
        let guard = AgentScopeGuard::banking_default();
        let message = Message::user("I need help with my account");
        let context = make_context("AuthenticatedCRM");
        let decision = guard.check_input(&message, &context).await;
        assert!(decision.is_pass());
    }
}
