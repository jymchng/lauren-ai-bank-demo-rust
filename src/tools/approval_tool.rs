use std::sync::Arc;
use std::time::Duration;

use agtrs::prelude::*;
use injectable::prelude::*;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::approval::service::ApprovalService;
use crate::signals::bus::AppSignalBus;

/// Tool to request human approval for a pending action.
#[injectable]
pub struct ApprovalTool {
    #[injectable(inject)]
    approval_service: Arc<ApprovalService>,
    #[injectable(inject)]
    signal_bus: Arc<AppSignalBus>,
}

#[async_trait::async_trait]
impl Tool for ApprovalTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "request_approval"
    }

    fn description(&self) -> &str {
        "Request user approval for a pending action (e.g., fund transfer)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action_type": { "type": "string", "description": "Type of action requiring approval" },
                "details": { "type": "string", "description": "JSON details of the action" }
            },
            "required": ["action_type", "details"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let conversation_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        let auth_uid = ctx
            .state
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let action_type = input
            .get("action_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let details_str = input
            .get("details")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let details: Value = serde_json::from_str(details_str).unwrap_or_default();
        let details_clone = details.clone();

        let (tx, rx) = oneshot::channel();
        self.approval_service
            .create_pending_approval(
                &conversation_id,
                &auth_uid,
                &action_type,
                details.clone(),
                tx,
            )
            .await;

        let to_user = details
            .get("to_user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let amount_usd = details
            .get("amount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let description = details
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(&action_type)
            .to_string();
        let created_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.signal_bus
            .emit(crate::signals::bus::AppSignal::ToolPendingApproval {
                approval_id: conversation_id.clone(),
                from_user: auth_uid.clone(),
                to_user,
                amount_usd,
                description,
                conversation_id: conversation_id.clone(),
                created_at_ms,
            });

        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(true)) => {
                self.approval_service
                    .mark_approved(&conversation_id, &details_clone)
                    .await;
                Ok(ToolResult::ok(
                    "Approval granted. You may proceed with the action.",
                    &ctx.tool_use_id,
                ))
            }
            Ok(Ok(false)) => Ok(ToolResult::ok(
                "Approval denied by the user. Do not proceed.",
                &ctx.tool_use_id,
            )),
            Ok(Err(_)) => Ok(ToolResult::ok(
                "Approval request was cancelled.",
                &ctx.tool_use_id,
            )),
            Err(_) => {
                self.approval_service
                    .cancel_approval(&conversation_id)
                    .await;
                Ok(ToolResult::ok(
                    "Approval request timed out after 30 seconds.",
                    &ctx.tool_use_id,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_state;

    async fn make_tool_ctx(user_id: &str, conversation_id: &str) -> ToolContext {
        let mut ctx = ToolContext::new("test_tool_call");
        if !user_id.is_empty() {
            ctx.state.insert("user_id".into(), json!(user_id));
        }
        ctx.state
            .insert("conversation_id".into(), json!(conversation_id));
        ctx
    }

    #[tokio::test]
    async fn test_approval_tool_name_and_schema() {
        let state = create_test_state().await;
        let tool: Arc<ApprovalTool> = state.container().resolve_external().await.unwrap();
        assert_eq!(tool.name(), "request_approval");
        assert!(tool.schema().is_object());
        assert!(tool.description().contains("approval"));
    }

    #[tokio::test]
    async fn test_approval_tool_creates_pending() {
        let state = create_test_state().await;
        let tool: Arc<ApprovalTool> = state.container().resolve_external().await.unwrap();
        let approval_service: Arc<ApprovalService> =
            state.container().resolve_external().await.unwrap();

        let ctx = make_tool_ctx("alice", "conv1").await;

        let tool_clone = Arc::clone(&tool);
        let ctx_clone = ctx.clone();
        let handle = tokio::spawn(async move {
            let _ = tool_clone
                .call(
                    json!({"action_type": "transfer", "details": "{\"to_user\":\"bob\",\"amount\":100}"}),
                    &ctx_clone,
                )
                .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(approval_service.has_pending("conv1").await);
        approval_service.respond("conv1", true).await.unwrap();
        let _ = handle.await;
    }
}
