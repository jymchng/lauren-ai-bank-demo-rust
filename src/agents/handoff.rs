use std::sync::Arc;

use agtrs::prelude::*;
use injectable::prelude::*;
use serde_json::{json, Value};

use crate::agents::active_agent_store::ActiveAgentStore;

// ── Helper to get ActiveAgentStore from DI context ────────────────────────────

async fn get_store(ctx: &ToolContext) -> Result<Arc<ActiveAgentStore>, AgtrsError> {
    ctx.resolve_context()
        .resolve_external::<Arc<ActiveAgentStore>>()
        .await
        .map_err(|e| AgtrsError::ToolCallFailed { tool_name: "dependency".into(), reason: format!("ActiveAgentStore unavailable: {e}") })
}

// ── HandoffToCrmTool ──────────────────────────────────────────────────────────

/// Tool to hand off to the Authenticated CRM agent.
#[injectable]
pub struct HandoffToCrmTool;

#[async_trait::async_trait]
impl Tool for HandoffToCrmTool {
    fn name(&self) -> &str { "handoff_to_crm" }
    fn description(&self) -> &str { "Hand off the conversation to the Authenticated CRM agent" }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let store = get_store(ctx).await?;
        let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("Agent handoff").to_string();
        let conv_id = ctx.state.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("default");
        store.set_active_agent(conv_id, "AuthenticatedCRM").await;
        store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(format!("Handed off to Authenticated CRM: {reason}"), &ctx.tool_use_id))
    }
}

// ── HandoffToTransferTool ─────────────────────────────────────────────────────

/// Tool to hand off to the Bank Transfer agent.
#[injectable]
pub struct HandoffToTransferTool;

#[async_trait::async_trait]
impl Tool for HandoffToTransferTool {
    fn name(&self) -> &str { "handoff_to_transfer" }
    fn description(&self) -> &str { "Hand off the conversation to the Bank Transfer agent" }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let store = get_store(ctx).await?;
        let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("Agent handoff").to_string();
        let conv_id = ctx.state.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("default");
        store.set_active_agent(conv_id, "BankTransfer").await;
        store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(format!("Handed off to Bank Transfer: {reason}"), &ctx.tool_use_id))
    }
}

// ── HandoffToDisputesTool ─────────────────────────────────────────────────────

/// Tool to hand off to the Disputes agent.
#[injectable]
pub struct HandoffToDisputesTool;

#[async_trait::async_trait]
impl Tool for HandoffToDisputesTool {
    fn name(&self) -> &str { "handoff_to_disputes" }
    fn description(&self) -> &str { "Hand off the conversation to the Disputes agent" }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let store = get_store(ctx).await?;
        let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("Agent handoff").to_string();
        let conv_id = ctx.state.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("default");
        store.set_active_agent(conv_id, "Disputes").await;
        store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(format!("Handed off to Disputes: {reason}"), &ctx.tool_use_id))
    }
}

// ── HandoffToAuthenticatedCrmTool ─────────────────────────────────────────────

/// Tool to hand off to the Authenticated CRM agent (requires authentication).
#[injectable]
pub struct HandoffToAuthenticatedCrmTool;

#[async_trait::async_trait]
impl Tool for HandoffToAuthenticatedCrmTool {
    fn name(&self) -> &str { "handoff_to_authenticated_crm" }
    fn description(&self) -> &str { "Hand off to the Authenticated CRM agent (requires authentication)" }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let user_id = ctx.state.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        if user_id.is_empty() {
            return Ok(ToolResult::ok(
                "Cannot hand off to authenticated CRM: user is not authenticated. Ask the user to log in first.",
                &ctx.tool_use_id,
            ));
        }
        let store = get_store(ctx).await?;
        let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("Agent handoff").to_string();
        let conv_id = ctx.state.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("default");
        store.set_active_agent(conv_id, "AuthenticatedCRM").await;
        store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(format!("Handed off to Authenticated CRM: {reason}"), &ctx.tool_use_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use injectable::prelude::*;

    async fn make_ctx(user_id: &str, conversation_id: &str) -> ToolContext {
        let container = Container::builder().build().await.unwrap();
        let resolve_ctx = Arc::new(container.context().clone());
        let mut ctx = ToolContext::new("test_tool_call", resolve_ctx);
        if !user_id.is_empty() {
            ctx.state.insert("user_id".into(), json!(user_id));
        }
        ctx.state.insert("conversation_id".into(), json!(conversation_id));
        ctx
    }

    async fn get_store_from_ctx(ctx: &ToolContext) -> Arc<ActiveAgentStore> {
        ctx.resolve_context()
            .resolve_external::<Arc<ActiveAgentStore>>().await.unwrap()
    }

    #[tokio::test]
    async fn test_handoff_to_crm() {
        let ctx = make_ctx("alice", "conv1").await;
        let store = get_store_from_ctx(&ctx).await;
        let result = HandoffToCrmTool.call(json!({"reason": "General help"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Authenticated CRM"));
        assert_eq!(store.get_active_agent("conv1").await, Some("AuthenticatedCRM".into()));
    }

    #[tokio::test]
    async fn test_handoff_to_transfer() {
        let ctx = make_ctx("alice", "conv1").await;
        let store = get_store_from_ctx(&ctx).await;
        let result = HandoffToTransferTool.call(json!({"reason": "Transfer needed"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(store.get_active_agent("conv1").await, Some("BankTransfer".into()));
    }

    #[tokio::test]
    async fn test_handoff_to_disputes() {
        let ctx = make_ctx("alice", "conv1").await;
        let store = get_store_from_ctx(&ctx).await;
        let result = HandoffToDisputesTool.call(json!({"reason": "Dispute"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(store.get_active_agent("conv1").await, Some("Disputes".into()));
    }

    #[tokio::test]
    async fn test_handoff_to_authenticated_crm_with_auth() {
        let ctx = make_ctx("alice", "conv1").await;
        let store = get_store_from_ctx(&ctx).await;
        let result = HandoffToAuthenticatedCrmTool.call(json!({"reason": "Authenticated"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(store.get_active_agent("conv1").await, Some("AuthenticatedCRM".into()));
    }

    #[tokio::test]
    async fn test_handoff_to_authenticated_crm_without_auth() {
        let ctx = make_ctx("", "conv1").await;
        let store = get_store_from_ctx(&ctx).await;
        let result = HandoffToAuthenticatedCrmTool.call(json!({"reason": "Unauthenticated"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("not authenticated"));
        assert_eq!(store.get_active_agent("conv1").await, None);
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(HandoffToCrmTool.name(), "handoff_to_crm");
        assert_eq!(HandoffToTransferTool.name(), "handoff_to_transfer");
        assert_eq!(HandoffToDisputesTool.name(), "handoff_to_disputes");
        assert_eq!(HandoffToAuthenticatedCrmTool.name(), "handoff_to_authenticated_crm");
    }
}
