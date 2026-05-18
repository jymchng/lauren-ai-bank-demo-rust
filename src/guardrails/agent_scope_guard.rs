use agtrs::agtrs_runtime;
//! Keyword-based agent scope guardrail.
//!
//! Blocks responses containing sensitive keywords that shouldn't be
//! shared by certain agent types.

use async_trait::async_trait;
use agtrs::agtrs_runtime::guardrail::{Guardrail, GuardrailContext, GuardrailDecision};
use agtrs::agtrs_runtime::transport::Message;
use agtrs::agtrs_runtime::llm::LlmResponse;
use agtrs::agtrs_runtime::tool::ToolResult;

/// Sensitive keywords that should never appear in agent responses.
const BLOCKED_KEYWORDS: &[&str] = &[
    "password",
    "secret_key",
    "api_key",
    "social_security",
    "ssn",
    "credit_card_number",
    "pin_number",
];

/// A keyword-based output guardrail that blocks responses containing
/// sensitive information.
pub struct AgentScopeGuard {
    /// Additional keywords to block beyond the defaults.
    extra_keywords: Vec<String>,
}

impl AgentScopeGuard {
    /// Create a new agent scope guard with default blocked keywords.
    pub fn new() -> Self {
        Self {
            extra_keywords: Vec::new(),
        }
    }

    /// Create with additional keywords to block.
    pub fn with_extra_keywords(keywords: Vec<String>) -> Self {
        Self {
            extra_keywords: keywords,
        }
    }
}

impl Default for AgentScopeGuard {
    fn default() -> Self {
        Self::new()
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

        for keyword in BLOCKED_KEYWORDS {
            if text.contains(keyword) {
                return GuardrailDecision::Block(format!(
                    "Response contains sensitive keyword: '{}'",
                    keyword
                ));
            }
        }

        for keyword in &self.extra_keywords {
            if text.contains(&keyword.to_lowercase()) {
                return GuardrailDecision::Block(format!(
                    "Response contains blocked keyword: '{}'",
                    keyword
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
        10 // Run first — security-critical
    }
}

#[cfg(test)]
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
    async fn test_blocks_password() {
        let guard = AgentScopeGuard::new();
        let response = make_response("Your password is hunter2");
        let ctx = GuardrailContext::new("test");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_blocks_ssn() {
        let guard = AgentScopeGuard::new();
        let response = make_response("Your social_security number is 123-45-6789");
        let ctx = GuardrailContext::new("test");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_passes_safe_response() {
        let guard = AgentScopeGuard::new();
        let response = make_response("Your balance is $5,000.");
        let ctx = GuardrailContext::new("test");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_pass());
    }

    #[tokio::test]
    async fn test_extra_keywords() {
        let guard =
            AgentScopeGuard::with_extra_keywords(vec!["internal_server".into()]);
        let response = make_response("Connect to internal_server for details.");
        let ctx = GuardrailContext::new("test");
        let decision = guard.check_output(&response, &ctx).await;
        assert!(decision.is_block());
    }

    #[tokio::test]
    async fn test_priority() {
        let guard = AgentScopeGuard::new();
        assert_eq!(guard.priority(), 10);
    }

    #[tokio::test]
    async fn test_default() {
        let guard = AgentScopeGuard::default();
        assert_eq!(guard.name(), "agent_scope_guard");
    }
}
