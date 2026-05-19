use std::sync::OnceLock;

use agtrs::prelude::*;
use injectable::prelude::*;
use serde_json::{json, Value};

use crate::knowledge::PublicKnowledgeBase;

static KB: OnceLock<PublicKnowledgeBase> = OnceLock::new();

fn knowledge_base() -> &'static PublicKnowledgeBase {
    KB.get_or_init(PublicKnowledgeBase::new)
}

/// Tool to search the public banking knowledge base.
#[injectable]
pub struct SearchPublicInfoTool;

#[async_trait::async_trait]
impl Tool for SearchPublicInfoTool {
    type Inputs = serde_json::Value;
    type Output = ToolResult;

    fn name(&self) -> &str {
        "search_public_info"
    }

    fn description(&self) -> &str {
        "Search the public banking knowledge base for information about products, fees, branches, and policies"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, AgtrsError> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if query.is_empty() {
            return Ok(ToolResult::error("Query is required", &ctx.tool_use_id));
        }

        let kb = knowledge_base();
        let results = kb.search(&query, 3);

        if results.is_empty() {
            return Ok(ToolResult::ok(
                "No relevant information found in the knowledge base for that query.",
                &ctx.tool_use_id,
            ));
        }

        let output: Vec<String> = results
            .iter()
            .map(|doc| {
                let preview = if doc.content.len() > 800 {
                    &doc.content[..800]
                } else {
                    &doc.content
                };
                format!("**{}**\n{}", doc.id, preview)
            })
            .collect();

        Ok(ToolResult::ok(output.join("\n\n---\n\n"), &ctx.tool_use_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> ToolContext {
        ToolContext::new("test-tool-use-id")
    }

    #[tokio::test]
    async fn test_search_found() {
        let tool = SearchPublicInfoTool;
        let ctx = make_ctx();
        let result = tool
            .call(json!({"query": "savings account"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_search_not_found() {
        let tool = SearchPublicInfoTool;
        let ctx = make_ctx();
        let result = tool
            .call(json!({"query": "quantum computing algorithms"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("No relevant"));
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let tool = SearchPublicInfoTool;
        let ctx = make_ctx();
        let result = tool.call(json!({"query": ""}), &ctx).await.unwrap();
        assert!(result.is_error);
    }
}
