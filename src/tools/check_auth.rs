use agtrs::prelude::*;
use injectable::prelude::*;
    use injectable_runtime::{EmptySingletonStore, ResolveContext};
use serde_json::{json, Value};

/// Tool to check if the current user is authenticated.
/// Zero-dep unit struct — resolves auth state from ToolContext.state at call time.
#[injectable]
pub struct CheckAuthenticationTool;

#[async_trait::async_trait]
impl Tool for CheckAuthenticationTool {
    fn name(&self) -> &str {
        "check_authentication"
    }

    fn description(&self) -> &str {
        "Check if the current user is authenticated"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let user_id = ctx
            .state
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if user_id.is_empty() {
            Ok(ToolResult::ok(
                "User is NOT authenticated. This is a public/unauthenticated session.",
                &ctx.tool_use_id,
            ))
        } else {
            Ok(ToolResult::ok(
                format!("User IS authenticated. User ID: {user_id}"),
                &ctx.tool_use_id,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use injectable::prelude::*;
    use injectable_runtime::{EmptySingletonStore, ResolveContext};

    fn make_ctx(user_id: Option<&str>) -> ToolContext {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        let mut ctx = ToolContext::new("test-tool-use-id", resolve_ctx);
        if let Some(uid) = user_id {
            ctx.state
                .insert("user_id".into(), serde_json::Value::String(uid.to_string()));
        }
        ctx
    }

    #[tokio::test]
    async fn test_check_auth_authenticated() {
        let tool = CheckAuthenticationTool;
        let ctx = make_ctx(Some("alice"));
        let result = tool.call(serde_json::json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("IS authenticated"));
    }

    #[tokio::test]
    async fn test_check_auth_unauthenticated() {
        let tool = CheckAuthenticationTool;
        let ctx = make_ctx(None);
        let result = tool.call(serde_json::json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("NOT authenticated"));
    }
}
