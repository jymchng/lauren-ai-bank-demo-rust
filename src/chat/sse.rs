use agtrs::agtrs_runtime;
//! SSE event mapping from StreamEvent to axum Event.

use axum::response::sse::Event;

use crate::chat::schemas::SseEvent;

impl From<SseEvent> for Event {
    fn from(event: SseEvent) -> Self {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Event::default().data(data)
    }
}

/// Convert an agtrs StreamEvent to an SSE event string.
pub fn stream_event_to_sse(event: &agtrs_runtime::streaming::StreamEvent) -> Option<SseEvent> {
    match event {
        agtrs_runtime::streaming::StreamEvent::TextDelta { delta } => Some(SseEvent::TextDelta {
            delta: delta.clone(),
        }),
        agtrs_runtime::streaming::StreamEvent::ToolExecution { tool_name, .. } => {
            Some(SseEvent::ToolExecution {
                tool_name: tool_name.clone(),
            })
        }
        agtrs_runtime::streaming::StreamEvent::PendingApproval {
            tool_name, ..
        } => Some(SseEvent::PendingApproval {
            action: format!("Approval needed for: {}", tool_name),
        }),
        agtrs_runtime::streaming::StreamEvent::Done {
            content,
            turns,
            ..
        } => Some(SseEvent::Done {
            content: content.clone(),
            conversation_id: String::new(),
            turns: *turns,
        }),
        agtrs_runtime::streaming::StreamEvent::Error { message } => Some(SseEvent::Error {
            message: message.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_to_axum_event() {
        let event = SseEvent::TextDelta {
            delta: "Hello".into(),
        };
        let axum_event: Event = event.into();
        // Just verify it converts without panic
        let _ = format!("{:?}", axum_event);
    }

    #[test]
    fn test_stream_event_text_delta() {
        let stream_event = agtrs_runtime::streaming::StreamEvent::TextDelta {
            delta: "Hi there".into(),
        };
        let sse = stream_event_to_sse(&stream_event);
        assert!(sse.is_some());
        match sse.unwrap() {
            SseEvent::TextDelta { delta } => assert_eq!(delta, "Hi there"),
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_stream_event_done() {
        let stream_event = agtrs_runtime::streaming::StreamEvent::Done {
            content: "Done!".into(),
            stop_reason: agtrs_runtime::transport::StopReason::EndTurn,
            usage: agtrs_runtime::transport::TokenUsage::new(10, 20),
            turns: 3,
        };
        let sse = stream_event_to_sse(&stream_event);
        assert!(sse.is_some());
        match sse.unwrap() {
            SseEvent::Done { content, turns, .. } => {
                assert_eq!(content, "Done!");
                assert_eq!(turns, 3);
            }
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_stream_event_error() {
        let stream_event = agtrs_runtime::streaming::StreamEvent::Error {
            message: "Oops".into(),
        };
        let sse = stream_event_to_sse(&stream_event);
        assert!(sse.is_some());
        match sse.unwrap() {
            SseEvent::Error { message } => assert_eq!(message, "Oops"),
            _ => panic!("Wrong type"),
        }
    }
}
