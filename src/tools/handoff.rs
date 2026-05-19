use std::sync::Arc;

use agtrs::prelude::*;
use injectable::prelude::*;
use serde_json::{json, Value};

use agtrs_runtime::team::HandoffAgentStore;

// ── HandoffToCrmTool ──────────────────────────────────────────────────────────

/// Tool to hand off to the Authenticated CRM agent.
#[injectable]
pub struct HandoffToCrmTool {
    #[injectable(inject(external))]
    store: Arc<HandoffAgentStore>,
}

#[async_trait::async_trait]
impl Tool for HandoffToCrmTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "handoff_to_crm"
    }
    fn description(&self) -> &str {
        "Hand off the conversation to the Authenticated CRM agent"
    }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Agent handoff")
            .to_string();
        let conv_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        self.store
            .set_active_agent(conv_id, "Banking CRM Agent (Authenticated)")
            .await;
        self.store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(
            format!("Handed off to Banking CRM Agent (Authenticated): {reason}"),
            &ctx.tool_use_id,
        ))
    }
}

// ── HandoffToTransferTool ─────────────────────────────────────────────────────

/// Tool to hand off to the Bank Transfer agent.
#[injectable]
pub struct HandoffToTransferTool {
    #[injectable(inject(external))]
    store: Arc<HandoffAgentStore>,
}

#[async_trait::async_trait]
impl Tool for HandoffToTransferTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "handoff_to_transfer"
    }
    fn description(&self) -> &str {
        "Hand off the conversation to the Bank Transfer agent"
    }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Agent handoff")
            .to_string();
        let conv_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        self.store
            .set_active_agent(conv_id, "Banking Transfer Agent")
            .await;
        self.store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(
            format!("Handed off to Banking Transfer Agent: {reason}"),
            &ctx.tool_use_id,
        ))
    }
}

// ── HandoffToDisputesTool ─────────────────────────────────────────────────────

/// Tool to hand off to the Disputes agent.
#[injectable]
pub struct HandoffToDisputesTool {
    #[injectable(inject(external))]
    store: Arc<HandoffAgentStore>,
}

#[async_trait::async_trait]
impl Tool for HandoffToDisputesTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "handoff_to_disputes"
    }
    fn description(&self) -> &str {
        "Hand off the conversation to the Disputes agent"
    }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Agent handoff")
            .to_string();
        let conv_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        self.store
            .set_active_agent(conv_id, "Banking Disputes Agent")
            .await;
        self.store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(
            format!("Handed off to Banking Disputes Agent: {reason}"),
            &ctx.tool_use_id,
        ))
    }
}

// ── HandoffToAuthenticatedCrmTool ─────────────────────────────────────────────

/// Tool to hand off to the Authenticated CRM agent (requires authentication).
#[injectable]
pub struct HandoffToAuthenticatedCrmTool {
    #[injectable(inject(external))]
    store: Arc<HandoffAgentStore>,
}

#[async_trait::async_trait]
impl Tool for HandoffToAuthenticatedCrmTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "handoff_to_authenticated_crm"
    }
    fn description(&self) -> &str {
        "Hand off to the Authenticated CRM agent (requires authentication)"
    }
    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": { "reason": { "type": "string" } }, "required": ["reason"] })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        // Prefer HTTP extensions (set by middleware) as authoritative auth source.
        // Fall back to ctx.state for non-HTTP contexts (tests, CLI).
        let is_authenticated = ctx
            .extensions
            .get::<crate::error::UserIdExtension>()
            .map(|e| !e.0.is_empty())
            .unwrap_or(false);

        if !is_authenticated {
            return Ok(ToolResult::ok(
                "Cannot hand off to authenticated CRM: user is not authenticated. Ask the user to log in first.",
                &ctx.tool_use_id,
            ));
        }
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Agent handoff")
            .to_string();
        let conv_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        self.store
            .set_active_agent(conv_id, "Banking CRM Agent (Authenticated)")
            .await;
        self.store.set_pending_summary(conv_id, &reason).await;
        Ok(ToolResult::ok(
            format!("Handed off to Banking CRM Agent (Authenticated): {reason}"),
            &ctx.tool_use_id,
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_state;

    async fn make_ctx(user_id: &str, conversation_id: &str) -> ToolContext {
        let mut ctx = ToolContext::new("test_tool_call");
        if !user_id.is_empty() {
            ctx.extensions
                .insert(crate::error::UserIdExtension(user_id.to_string()));
        }
        ctx.state
            .insert("conversation_id".into(), json!(conversation_id));
        ctx
    }

    #[tokio::test]
    async fn test_handoff_to_crm() {
        let state = create_test_state().await;
        let tool: Arc<HandoffToCrmTool> = state.container().resolve_external().await.unwrap();
        let store: Arc<HandoffAgentStore> = state.container().resolve_external().await.unwrap();
        let ctx = make_ctx("alice", "conv1").await;
        let result = tool
            .call(json!({"reason": "General help"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Banking CRM Agent (Authenticated)"));
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("Banking CRM Agent (Authenticated)".into())
        );
    }

    #[tokio::test]
    async fn test_handoff_to_transfer() {
        let state = create_test_state().await;
        let tool: Arc<HandoffToTransferTool> = state.container().resolve_external().await.unwrap();
        let store: Arc<HandoffAgentStore> = state.container().resolve_external().await.unwrap();
        let ctx = make_ctx("alice", "conv1").await;
        let result = tool
            .call(json!({"reason": "Transfer needed"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("Banking Transfer Agent".into())
        );
    }

    #[tokio::test]
    async fn test_handoff_to_disputes() {
        let state = create_test_state().await;
        let tool: Arc<HandoffToDisputesTool> = state.container().resolve_external().await.unwrap();
        let store: Arc<HandoffAgentStore> = state.container().resolve_external().await.unwrap();
        let ctx = make_ctx("alice", "conv1").await;
        let result = tool.call(json!({"reason": "Dispute"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("Banking Disputes Agent".into())
        );
    }

    #[tokio::test]
    async fn test_handoff_to_authenticated_crm_with_auth() {
        let state = create_test_state().await;
        let tool: Arc<HandoffToAuthenticatedCrmTool> =
            state.container().resolve_external().await.unwrap();
        let store: Arc<HandoffAgentStore> = state.container().resolve_external().await.unwrap();
        let ctx = make_ctx("alice", "conv1").await;
        let result = tool
            .call(json!({"reason": "Authenticated"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("Banking CRM Agent (Authenticated)".into())
        );
    }

    #[tokio::test]
    async fn test_handoff_to_authenticated_crm_without_auth() {
        let state = create_test_state().await;
        let tool: Arc<HandoffToAuthenticatedCrmTool> =
            state.container().resolve_external().await.unwrap();
        let store: Arc<HandoffAgentStore> = state.container().resolve_external().await.unwrap();
        let ctx = make_ctx("", "conv1").await;
        let result = tool
            .call(json!({"reason": "Unauthenticated"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("not authenticated"));
        assert_eq!(store.get_active_agent("conv1").await, None);
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(
            HandoffToCrmTool {
                store: Arc::new(HandoffAgentStore::new())
            }
            .name(),
            "handoff_to_crm"
        );
        assert_eq!(
            HandoffToTransferTool {
                store: Arc::new(HandoffAgentStore::new())
            }
            .name(),
            "handoff_to_transfer"
        );
        assert_eq!(
            HandoffToDisputesTool {
                store: Arc::new(HandoffAgentStore::new())
            }
            .name(),
            "handoff_to_disputes"
        );
        assert_eq!(
            HandoffToAuthenticatedCrmTool {
                store: Arc::new(HandoffAgentStore::new())
            }
            .name(),
            "handoff_to_authenticated_crm"
        );
    }
}
