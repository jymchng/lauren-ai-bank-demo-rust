/// Pattern A demo: `#[tool]` on an impl block with `#[agtrs(tool_run)]` method marker.
///
/// This verifies end-to-end macro expansion: docstring description, typed parameters,
/// #[agtrs(tool_param(...))] annotations, ToolContext pass-through, and generated schema.
use agtrs::prelude::*;
use injectable::prelude::*;

/// Summary of account activity for an authenticated user.
#[injectable]
pub struct AccountSummaryTool;

/// Get a summary of recent account activity.
#[tool]
impl AccountSummaryTool {
    /// Summarize recent account transactions and balance for the given account type.
    #[agtrs(tool_run)]
    pub async fn run(
        &self,
        #[agtrs(tool_param(
            description = "Account type: checking or savings",
            default = "checking"
        ))]
        account_type: String,
        #[agtrs(tool_param(description = "Number of recent transactions to include"))] limit: u32,
        ctx: &ToolContext,
    ) -> Result<ToolResult, AgtrsError> {
        let user_id_owned = ctx
            .extensions
            .get::<crate::error::UserIdExtension>()
            .map(|e| e.0.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let user_id = user_id_owned.as_str();

        Ok(ToolResult::ok(
            format!(
                "Account summary for user '{user_id}': {account_type} account, last {limit} transactions."
            ),
            &ctx.tool_use_id,
        ))
    }
}

mod tests {
    use super::*;
    use agtrs::prelude::*;

    #[tokio::test]
    async fn account_summary_tool_schema_has_required_fields() {
        let tool = AccountSummaryTool;
        let schema = tool.schema();

        // schemars generates a full JSON Schema — check required properties exist
        let schema_str = serde_json::to_string(&schema).unwrap();
        assert!(
            schema_str.contains("account_type"),
            "schema must reference account_type"
        );
        assert!(schema_str.contains("limit"), "schema must reference limit");
    }

    #[tokio::test]
    async fn account_summary_tool_call_with_valid_input() {
        let tool = AccountSummaryTool;
        let mut ctx = ToolContext::new("test-id");
        ctx.extensions
            .insert(crate::error::UserIdExtension("alice".to_string()));

        let result = tool
            .call(
                serde_json::json!({"account_type": "savings", "limit": 5}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("alice"));
        assert!(result.content.contains("savings"));
        assert!(result.content.contains("5"));
    }

    #[tokio::test]
    async fn account_summary_tool_name_and_description() {
        let tool = AccountSummaryTool;
        assert_eq!(tool.name(), "account_summary_tool");
        assert!(
            !tool.description().is_empty(),
            "description should come from docstring"
        );
    }
}
