use std::sync::Arc;

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::approval::service::ApprovalService;
use crate::banking::db::BankDatabase;
use crate::signals::bus::{AppSignal, AppSignalBus};

// ── GetBalanceTool ────────────────────────────────────────────────────────────

/// Tool to get the account balance for the authenticated user.
#[injectable]
pub struct GetBalanceTool {
    #[injectable(inject)]
    db: Arc<BankDatabase>,
}

#[tool(name = "get_balance")]
impl GetBalanceTool {
    /// Get the account balance for the authenticated user.
    #[agtrs(tool_run)]
    pub async fn run(
        &self,
        #[agtrs(tool_param(description = "The user ID the customer identified themselves as"))]
        user_id: String,
        ctx: &ToolContext,
    ) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx
            .state
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if auth_uid.is_empty() {
            return Ok(ToolResult::error(
                "User not authenticated",
                &ctx.tool_use_id,
            ));
        }
        if user_id != auth_uid {
            return Ok(ToolResult::error(
                format!("Access denied: authenticated as '{auth_uid}', not '{user_id}'"),
                &ctx.tool_use_id,
            ));
        }
        match self.db.get_balance(auth_uid).await {
            Some(balance) => Ok(ToolResult::ok(
                format!("Balance: ${:.2}", balance),
                &ctx.tool_use_id,
            )),
            None => Ok(ToolResult::error("Account not found", &ctx.tool_use_id)),
        }
    }
}

// ── TransferFundsTool ─────────────────────────────────────────────────────────

/// Tool to transfer funds between accounts (requires one-shot approval).
#[injectable]
pub struct TransferFundsTool {
    #[injectable(inject)]
    db: Arc<BankDatabase>,
    #[injectable(inject)]
    approval_svc: Arc<ApprovalService>,
    #[injectable(inject)]
    signal_bus: Arc<AppSignalBus>,
}

#[tool(name = "transfer_funds", requires_confirmation = true)]
impl TransferFundsTool {
    /// Transfer funds from the authenticated user's account to another user.
    #[agtrs(tool_run)]
    pub async fn run(
        &self,
        #[agtrs(tool_param(description = "The user ID the customer identified themselves as"))]
        user_id: String,
        #[agtrs(tool_param(description = "The recipient user ID"))] to_user: String,
        #[agtrs(tool_param(description = "The amount to transfer in USD"))] amount: f64,
        ctx: &ToolContext,
    ) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx
            .state
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if auth_uid.is_empty() {
            return Ok(ToolResult::error(
                "User not authenticated",
                &ctx.tool_use_id,
            ));
        }
        if user_id != auth_uid {
            return Ok(ToolResult::error(
                format!("Access denied: authenticated as '{auth_uid}', not '{user_id}'"),
                &ctx.tool_use_id,
            ));
        }

        if to_user.is_empty() {
            return Ok(ToolResult::error(
                "Recipient user ID is required",
                &ctx.tool_use_id,
            ));
        }
        if amount <= 0.0 {
            return Ok(ToolResult::error(
                "Transfer amount must be positive",
                &ctx.tool_use_id,
            ));
        }

        let conv_id = ctx
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let token = self.approval_svc.get_approved_transfer(&conv_id).await;
        let approved = token
            .as_ref()
            .map(|t| {
                let ok_to = t.get("to_user").and_then(|v| v.as_str()).unwrap_or("");
                let ok_amount = t.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                ok_to == to_user && (ok_amount - amount).abs() < 0.01
            })
            .unwrap_or(false);

        if !approved {
            return Ok(ToolResult::ok(
                "Error: Transfer requires prior approval. Use the approval tool first.",
                &ctx.tool_use_id,
            ));
        }

        match self.db.transfer(auth_uid, &to_user, amount).await {
            Ok(tx) => {
                let from_balance = self.db.get_balance(auth_uid).await.unwrap_or(0.0);
                let to_balance = self.db.get_balance(&to_user).await.unwrap_or(0.0);
                self.signal_bus.emit(AppSignal::BalanceChanged {
                    from_user: auth_uid.to_string(),
                    to_user: to_user.clone(),
                    amount,
                    from_balance,
                    to_balance,
                });
                Ok(ToolResult::ok(
                    format!("Transfer successful: {}", tx.description),
                    &ctx.tool_use_id,
                ))
            }
            Err(e) => Ok(ToolResult::error(
                format!("Transfer failed: {}", e),
                &ctx.tool_use_id,
            )),
        }
    }
}

// ── GetTransactionHistoryTool ─────────────────────────────────────────────────

/// Tool to get transaction history for the authenticated user.
#[injectable]
pub struct GetTransactionHistoryTool {
    #[injectable(inject)]
    db: Arc<BankDatabase>,
}

#[tool(name = "get_transaction_history")]
impl GetTransactionHistoryTool {
    /// Get transaction history for the authenticated user.
    #[agtrs(tool_run)]
    pub async fn run(
        &self,
        #[agtrs(tool_param(description = "The user ID the customer identified themselves as"))]
        user_id: String,
        ctx: &ToolContext,
    ) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx
            .state
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if auth_uid.is_empty() {
            return Ok(ToolResult::error(
                "User not authenticated",
                &ctx.tool_use_id,
            ));
        }
        if user_id != auth_uid {
            return Ok(ToolResult::error(
                format!("Access denied: authenticated as '{auth_uid}', not '{user_id}'"),
                &ctx.tool_use_id,
            ));
        }

        let transactions = self.db.get_transactions(auth_uid).await;
        if transactions.is_empty() {
            return Ok(ToolResult::ok("No transactions found.", &ctx.tool_use_id));
        }

        let output: Vec<String> = transactions
            .iter()
            .map(|tx| {
                format!(
                    "{}: {} (${:.2}) - {}",
                    tx.timestamp, tx.description, tx.amount, tx.to_name
                )
            })
            .collect();

        Ok(ToolResult::ok(output.join("\n"), &ctx.tool_use_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::bus::AppSignalBus;
    use serde_json::json;

    fn make_db() -> Arc<BankDatabase> {
        Arc::new(BankDatabase::new())
    }

    fn make_approval_svc() -> Arc<ApprovalService> {
        Arc::new(ApprovalService::new(Arc::new(AppSignalBus::new())))
    }

    fn make_signal_bus() -> Arc<AppSignalBus> {
        Arc::new(AppSignalBus::new())
    }

    fn make_ctx(user_id: &str) -> ToolContext {
        let mut ctx = ToolContext::new("test_tool_call");
        if !user_id.is_empty() {
            ctx.state.insert("user_id".into(), json!(user_id));
        }
        ctx.state
            .insert("conversation_id".into(), json!("test-conv"));
        ctx
    }

    async fn make_transfer_tool_with_approval(
        to_user: &str,
        amount: f64,
    ) -> (TransferFundsTool, ToolContext) {
        let approval_svc = make_approval_svc();
        approval_svc
            .mark_approved("test-conv", &json!({"to_user": to_user, "amount": amount}))
            .await;
        let tool = TransferFundsTool {
            db: make_db(),
            approval_svc,
            signal_bus: make_signal_bus(),
        };
        (tool, make_ctx("alice"))
    }

    // ── GetBalanceTool ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_balance_authenticated() {
        let tool = GetBalanceTool { db: make_db() };
        let ctx = make_ctx("alice");
        let result = tool.call(json!({"user_id": "alice"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("5000"));
    }

    #[tokio::test]
    async fn test_get_balance_unauthenticated() {
        let tool = GetBalanceTool { db: make_db() };
        let ctx = make_ctx("");
        let result = tool.call(json!({"user_id": "alice"}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not authenticated"));
    }

    #[tokio::test]
    async fn test_get_balance_mismatched_user_id() {
        let tool = GetBalanceTool { db: make_db() };
        let ctx = make_ctx("alice");
        let result = tool.call(json!({"user_id": "bob"}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Access denied"));
        assert!(result.content.contains("alice"));
    }

    #[tokio::test]
    async fn test_get_balance_unknown_user() {
        let tool = GetBalanceTool { db: make_db() };
        let ctx = make_ctx("unknown");
        let result = tool
            .call(json!({"user_id": "unknown"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    // ── TransferFundsTool ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_transfer_funds_without_approval() {
        let tool = TransferFundsTool {
            db: make_db(),
            approval_svc: make_approval_svc(),
            signal_bus: make_signal_bus(),
        };
        let ctx = make_ctx("alice");
        let result = tool
            .call(
                json!({"user_id": "alice", "to_user": "bob", "amount": 100}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("approval"));
    }

    #[tokio::test]
    async fn test_transfer_funds_with_approval() {
        let (tool, ctx) = make_transfer_tool_with_approval("bob", 100.0).await;
        let result = tool
            .call(
                json!({"user_id": "alice", "to_user": "bob", "amount": 100}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("successful"));
    }

    #[tokio::test]
    async fn test_transfer_funds_unauthenticated() {
        let tool = TransferFundsTool {
            db: make_db(),
            approval_svc: make_approval_svc(),
            signal_bus: make_signal_bus(),
        };
        let ctx = make_ctx("");
        let result = tool
            .call(
                json!({"user_id": "alice", "to_user": "bob", "amount": 100}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not authenticated"));
    }

    #[tokio::test]
    async fn test_transfer_funds_mismatched_user_id() {
        let tool = TransferFundsTool {
            db: make_db(),
            approval_svc: make_approval_svc(),
            signal_bus: make_signal_bus(),
        };
        let ctx = make_ctx("alice");
        let result = tool
            .call(
                json!({"user_id": "charlie", "to_user": "bob", "amount": 100}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Access denied"));
    }

    // ── GetTransactionHistoryTool ───────────────────────────────────────────

    #[tokio::test]
    async fn test_transaction_history_no_transactions() {
        let tool = GetTransactionHistoryTool { db: make_db() };
        let ctx = make_ctx("alice");
        let result = tool.call(json!({"user_id": "alice"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("No transactions"));
    }

    #[tokio::test]
    async fn test_transaction_history_unauthenticated() {
        let tool = GetTransactionHistoryTool { db: make_db() };
        let ctx = make_ctx("");
        let result = tool.call(json!({"user_id": "alice"}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not authenticated"));
    }

    #[tokio::test]
    async fn test_transaction_history_mismatched_user_id() {
        let tool = GetTransactionHistoryTool { db: make_db() };
        let ctx = make_ctx("alice");
        let result = tool.call(json!({"user_id": "bob"}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Access denied"));
    }

    // ── Schema / metadata ──────────────────────────────────────────────────

    #[test]
    fn test_tool_names_and_schemas() {
        let db = make_db();
        let approval_svc = make_approval_svc();
        let signal_bus = make_signal_bus();

        let balance_tool = GetBalanceTool {
            db: Arc::clone(&db),
        };
        assert_eq!(balance_tool.name(), "get_balance");
        assert!(!balance_tool.description().is_empty());
        let schema = balance_tool.schema();
        assert!(schema.is_object());
        assert!(schema["properties"]["user_id"].is_object());

        let transfer_tool = TransferFundsTool {
            db: Arc::clone(&db),
            approval_svc: Arc::clone(&approval_svc),
            signal_bus: Arc::clone(&signal_bus),
        };
        assert_eq!(transfer_tool.name(), "transfer_funds");
        assert!(transfer_tool.requires_confirmation());
        let schema = transfer_tool.schema();
        assert!(schema["properties"]["user_id"].is_object());
        assert!(schema["properties"]["to_user"].is_object());
        assert!(schema["properties"]["amount"].is_object());

        let history_tool = GetTransactionHistoryTool {
            db: Arc::clone(&db),
        };
        assert_eq!(history_tool.name(), "get_transaction_history");
        let schema = history_tool.schema();
        assert!(schema["properties"]["user_id"].is_object());
    }
}
