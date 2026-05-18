use std::sync::Arc;

use agtrs::prelude::*;
use injectable::prelude::*;
use serde_json::{json, Value};

use crate::banking::db::BankDatabase;

// ── GetBalanceTool ────────────────────────────────────────────────────────────

/// Tool to get the account balance for the authenticated user.
/// Zero-dep unit struct — resolves BankDatabase from the DI container at call time.
#[injectable]
pub struct GetBalanceTool;

#[async_trait::async_trait]
impl Tool for GetBalanceTool {
    fn name(&self) -> &str { "get_balance" }

    fn description(&self) -> &str {
        "Get the account balance for the authenticated user"
    }

    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "required": [] })
    }

    async fn call(&self, _input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx.state.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        if auth_uid.is_empty() {
            return Ok(ToolResult::error("User not authenticated", &ctx.tool_use_id));
        }

        let db: Arc<BankDatabase> = ctx.resolve_context()
            .resolve_external::<Arc<BankDatabase>>()
            .await
            .map_err(|e| AgtrsError::ToolCallFailed { tool_name: "dependency".into(), reason: format!("BankDatabase unavailable: {e}") })?;

        match db.get_balance(auth_uid).await {
            Some(balance) => Ok(ToolResult::ok(format!("Balance: ${:.2}", balance), &ctx.tool_use_id)),
            None => Ok(ToolResult::error("Account not found", &ctx.tool_use_id)),
        }
    }
}

// ── TransferFundsTool ─────────────────────────────────────────────────────────

/// Tool to transfer funds between accounts (requires one-shot approval).
/// Zero-dep unit struct — resolves BankDatabase from the DI container at call time.
#[injectable]
pub struct TransferFundsTool;

#[async_trait::async_trait]
impl Tool for TransferFundsTool {
    fn name(&self) -> &str { "transfer_funds" }

    fn description(&self) -> &str {
        "Transfer funds from the authenticated user's account to another user"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "to_user": { "type": "string", "description": "The recipient user ID" },
                "amount":  { "type": "number", "description": "The amount to transfer" }
            },
            "required": ["to_user", "amount"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx.state.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if auth_uid.is_empty() {
            return Ok(ToolResult::error("User not authenticated", &ctx.tool_use_id));
        }

        let to_user = input.get("to_user").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let amount  = input.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);

        if to_user.is_empty() {
            return Ok(ToolResult::error("Recipient user ID is required", &ctx.tool_use_id));
        }
        if amount <= 0.0 {
            return Ok(ToolResult::error("Transfer amount must be positive", &ctx.tool_use_id));
        }

        let approved = ctx.state.get("transfer_approved")
            .and_then(|v| v.as_object())
            .map(|obj| {
                let ok_to     = obj.get("to_user").and_then(|v| v.as_str()).unwrap_or("");
                let ok_amount = obj.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let consumed  = obj.get("consumed").and_then(|v| v.as_bool()).unwrap_or(true);
                ok_to == to_user && (ok_amount - amount).abs() < 0.01 && !consumed
            })
            .unwrap_or(false);

        if !approved {
            return Ok(ToolResult::ok(
                "Error: Transfer requires prior approval. Use the approval tool first.",
                &ctx.tool_use_id,
            ));
        }

        let db: Arc<BankDatabase> = ctx.resolve_context()
            .resolve_external::<Arc<BankDatabase>>()
            .await
            .map_err(|e| AgtrsError::ToolCallFailed { tool_name: "dependency".into(), reason: format!("BankDatabase unavailable: {e}") })?;

        match db.transfer(&auth_uid, &to_user, amount).await {
            Ok(tx)  => Ok(ToolResult::ok(format!("Transfer successful: {}", tx.description), &ctx.tool_use_id)),
            Err(e)  => Ok(ToolResult::error(format!("Transfer failed: {}", e), &ctx.tool_use_id)),
        }
    }
}

// ── GetTransactionHistoryTool ─────────────────────────────────────────────────

/// Tool to get transaction history for the authenticated user.
/// Zero-dep unit struct — resolves BankDatabase from the DI container at call time.
#[injectable]
pub struct GetTransactionHistoryTool;

#[async_trait::async_trait]
impl Tool for GetTransactionHistoryTool {
    fn name(&self) -> &str { "get_transaction_history" }

    fn description(&self) -> &str {
        "Get transaction history for the authenticated user"
    }

    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "required": [] })
    }

    async fn call(&self, _input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let auth_uid = ctx.state.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        if auth_uid.is_empty() {
            return Ok(ToolResult::error("User not authenticated", &ctx.tool_use_id));
        }

        let db: Arc<BankDatabase> = ctx.resolve_context()
            .resolve_external::<Arc<BankDatabase>>()
            .await
            .map_err(|e| AgtrsError::ToolCallFailed { tool_name: "dependency".into(), reason: format!("BankDatabase unavailable: {e}") })?;

        let transactions = db.get_transactions(auth_uid).await;
        if transactions.is_empty() {
            return Ok(ToolResult::ok("No transactions found.", &ctx.tool_use_id));
        }

        let output: Vec<String> = transactions.iter()
            .map(|tx| format!("{}: {} (${:.2}) - {}", tx.timestamp, tx.description, tx.amount, tx.to_name))
            .collect();

        Ok(ToolResult::ok(output.join("\n"), &ctx.tool_use_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use injectable::prelude::*;
    use std::collections::HashMap;

    /// Build a ToolContext backed by a real injectable container that has BankDatabase.
    async fn make_ctx_with_db(user_id: &str) -> (ToolContext, Arc<BankDatabase>) {
        let container = Container::builder().build().await.unwrap();
        let db: Arc<BankDatabase> = container
            .resolve_external::<Arc<BankDatabase>>()
            .await
            .unwrap();
        let resolve_ctx = Arc::new(container.context().clone());
        let mut ctx = ToolContext::new("test_tool_call", resolve_ctx);
        if !user_id.is_empty() {
            ctx.state.insert("user_id".into(), json!(user_id));
        }
        (ctx, db)
    }

    async fn make_ctx_with_approval(user_id: &str, to_user: &str, amount: f64) -> ToolContext {
        let (mut ctx, _) = make_ctx_with_db(user_id).await;
        ctx.state.insert("transfer_approved".into(), json!({
            "to_user": to_user, "amount": amount, "consumed": false
        }));
        ctx
    }

    #[tokio::test]
    async fn test_get_balance_authenticated() {
        let (ctx, _) = make_ctx_with_db("alice").await;
        let result = GetBalanceTool.call(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("5000"));
    }

    #[tokio::test]
    async fn test_get_balance_unauthenticated() {
        let (ctx, _) = make_ctx_with_db("").await;
        let result = GetBalanceTool.call(json!({}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not authenticated"));
    }

    #[tokio::test]
    async fn test_get_balance_unknown_user() {
        let (ctx, _) = make_ctx_with_db("unknown").await;
        let result = GetBalanceTool.call(json!({}), &ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_transfer_funds_without_approval() {
        let (ctx, _) = make_ctx_with_db("alice").await;
        let result = TransferFundsTool.call(json!({"to_user": "bob", "amount": 100}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("approval"));
    }

    #[tokio::test]
    async fn test_transfer_funds_with_approval() {
        let ctx = make_ctx_with_approval("alice", "bob", 100.0).await;
        let result = TransferFundsTool.call(json!({"to_user": "bob", "amount": 100}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("successful"));
    }

    #[tokio::test]
    async fn test_transfer_funds_unauthenticated() {
        let (ctx, _) = make_ctx_with_db("").await;
        let result = TransferFundsTool.call(json!({"to_user": "bob", "amount": 100}), &ctx).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_transaction_history_no_transactions() {
        let (ctx, _) = make_ctx_with_db("alice").await;
        let result = GetTransactionHistoryTool.call(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("No transactions"));
    }

    #[tokio::test]
    async fn test_transaction_history_unauthenticated() {
        let (ctx, _) = make_ctx_with_db("").await;
        let result = GetTransactionHistoryTool.call(json!({}), &ctx).await.unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn test_tool_names_and_schemas() {
        assert_eq!(GetBalanceTool.name(), "get_balance");
        assert!(!GetBalanceTool.description().is_empty());
        assert!(GetBalanceTool.schema().is_object());

        assert_eq!(TransferFundsTool.name(), "transfer_funds");
        assert!(TransferFundsTool.schema().get("required").is_some());

        assert_eq!(GetTransactionHistoryTool.name(), "get_transaction_history");
    }
}
