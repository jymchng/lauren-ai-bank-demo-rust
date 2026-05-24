//! Chat request/response schemas.

use serde::{Deserialize, Serialize};

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Chat request body — matches Python ChatRequest schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Conversation history; last user message is the current input.
    pub messages: Vec<ChatMessage>,
    /// LLM model hint (informational; Rust uses its configured model).
    #[serde(default)]
    pub model: Option<String>,
    /// Conversation ID for session continuity.
    pub conversation_id: Option<String>,
    /// Optional user ID (for authenticated requests).
    pub user_id: Option<String>,
}

impl ChatRequest {
    /// Extract the content of the last user message.
    pub fn last_user_message(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role == "user")
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default()
    }
}

/// Chat response body (for non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub conversation_id: String,
    pub agent_name: String,
    pub turns: usize,
}

mod tests {
    use super::*;

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"messages":[{"role":"user","content":"What's my balance?"}],"conversation_id":"conv-1","user_id":"user-123"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.last_user_message(), "What's my balance?");
        assert_eq!(req.conversation_id, Some("conv-1".into()));
    }

    #[test]
    fn test_chat_request_optional_fields() {
        let json = r#"{"messages":[{"role":"user","content":"hello"}]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.last_user_message(), "hello");
        assert!(req.conversation_id.is_none());
        assert!(req.user_id.is_none());
    }

    #[test]
    fn test_last_user_message_picks_last() {
        let json = r#"{"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"reply"},{"role":"user","content":"second"}]}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.last_user_message(), "second");
    }

    #[test]
    fn test_chat_response_serialization() {
        let resp = ChatResponse {
            response: "Your balance is $5,000.".into(),
            conversation_id: "conv-1".into(),
            agent_name: "Banking CRM Agent (Authenticated)".into(),
            turns: 2,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Banking CRM Agent (Authenticated)"));
    }
}
