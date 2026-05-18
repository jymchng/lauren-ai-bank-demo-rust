//! Chat request/response schemas and SSE event types.

use serde::{Deserialize, Serialize};

/// Chat request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The user's message.
    pub message: String,
    /// The conversation ID (for session continuity).
    pub conversation_id: Option<String>,
    /// Optional user ID (for authenticated requests).
    pub user_id: Option<String>,
}

/// Chat response body (for non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The assistant's response.
    pub response: String,
    /// The conversation ID.
    pub conversation_id: String,
    /// The agent that handled the request.
    pub agent_name: String,
    /// Number of turns executed.
    pub turns: usize,
}

/// SSE event types for streaming responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    /// A text delta from the agent.
    #[serde(rename = "text_delta")]
    TextDelta {
        /// Incremental text content.
        delta: String,
    },
    /// A tool is being executed.
    #[serde(rename = "tool_execution")]
    ToolExecution {
        /// The tool name.
        tool_name: String,
    },
    /// The agent is switching to another agent.
    #[serde(rename = "agent_handoff")]
    AgentHandoff {
        /// The source agent.
        from: String,
        /// The target agent.
        to: String,
        /// Handoff summary.
        summary: String,
    },
    /// The agent needs approval for an action.
    #[serde(rename = "pending_approval")]
    PendingApproval {
        /// Description of the action.
        action: String,
    },
    /// The response is complete.
    #[serde(rename = "done")]
    Done {
        /// Final response content.
        content: String,
        /// The conversation ID.
        conversation_id: String,
        /// Number of turns.
        turns: usize,
    },
    /// An error occurred.
    #[serde(rename = "error")]
    Error {
        /// Error message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_deserialization() {
        let json = r#"{"message":"What's my balance?","conversation_id":"conv-1","user_id":"user-123"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "What's my balance?");
        assert_eq!(req.conversation_id, Some("conv-1".into()));
    }

    #[test]
    fn test_chat_response_serialization() {
        let resp = ChatResponse {
            response: "Your balance is $5,000.".into(),
            conversation_id: "conv-1".into(),
            agent_name: "authenticated_crm".into(),
            turns: 2,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("authenticated_crm"));
    }

    #[test]
    fn test_sse_event_serialization() {
        let event = SseEvent::TextDelta {
            delta: "Hello".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("text_delta"));

        let event = SseEvent::Done {
            content: "Done".into(),
            conversation_id: "conv-1".into(),
            turns: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("done"));
    }

    #[test]
    fn test_sse_event_deserialization() {
        let json = r#"{"type":"error","message":"Something went wrong"}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();
        match event {
            SseEvent::Error { message } => assert_eq!(message, "Something went wrong"),
            _ => panic!("Wrong event type"),
        }
    }
}
