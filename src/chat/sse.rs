//! SSE event mapping — converts agtrs StreamEvent to axum SSE Event.
//!
//! Uses Python-compatible event names: `token`, `tool_use`, `break`, `done`,
//! `error`, `guardrail_override`. Data is plain text, not JSON.

use agtrs_runtime::streaming::StreamEvent;
use axum::response::sse::Event;

/// Convert an agtrs StreamEvent to an axum SSE Event using Python-compatible names.
///
/// Returns `None` for events that are not forwarded to clients.
pub fn stream_event_to_sse(event: &StreamEvent) -> Option<Event> {
    match event {
        StreamEvent::TextDelta { delta } => {
            Some(Event::default().event("token").data(delta.clone()))
        }
        StreamEvent::ToolExecution { tool_name, .. } => {
            Some(Event::default().event("tool_use").data(tool_name.clone()))
        }
        StreamEvent::Done { .. } => None, // chat loop emits final "done" after all agents complete
        StreamEvent::Error { message } => {
            Some(Event::default().event("error").data(message.clone()))
        }
        StreamEvent::GuardrailOverride { content } => Some(
            Event::default()
                .event("guardrail_override")
                .data(content.clone()),
        ),
        // ThinkingDelta, ToolCallDelta, ToolResult, PendingApproval — not forwarded to client
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agtrs_runtime::transport::{StopReason, TokenUsage};

    #[test]
    fn test_text_delta_maps_to_token() {
        let event = StreamEvent::TextDelta {
            delta: "Hello".into(),
        };
        assert!(stream_event_to_sse(&event).is_some());
    }

    #[test]
    fn test_tool_execution_maps_to_tool_use() {
        let event = StreamEvent::ToolExecution {
            tool_name: "get_balance".into(),
            tool_use_id: "tu-1".into(),
        };
        assert!(stream_event_to_sse(&event).is_some());
    }

    #[test]
    fn test_done_suppressed() {
        let event = StreamEvent::Done {
            content: "Done!".into(),
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::new(10, 20),
            turns: 3,
            agent_name: "test_agent".into(),
            messages: vec![],
        };
        assert!(stream_event_to_sse(&event).is_none());
    }

    #[test]
    fn test_error_forwarded() {
        let event = StreamEvent::Error {
            message: "Oops".into(),
        };
        assert!(stream_event_to_sse(&event).is_some());
    }

    #[test]
    fn test_tool_result_suppressed() {
        let event = StreamEvent::ToolResult {
            result: agtrs_runtime::tool::ToolResult {
                tool_use_id: "tu-1".into(),
                content: "ok".into(),
                is_error: false,
            },
        };
        assert!(stream_event_to_sse(&event).is_none());
    }

    #[test]
    fn test_thinking_delta_suppressed() {
        let event = StreamEvent::ThinkingDelta {
            delta: "thinking...".into(),
        };
        assert!(stream_event_to_sse(&event).is_none());
    }
}
