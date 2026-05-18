//! Chat REST routes — authenticated and public SSE streaming endpoints.

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::{self, Stream};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use crate::chat::schemas::{ChatRequest, ChatResponse, SseEvent};
use crate::error::AppError;
use crate::AppState;

/// POST /api/banking/chat — authenticated chat endpoint (SSE streaming).
pub async fn chat_authenticated(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>, AppError> {
    let user_id = req
        .user_id
        .clone()
        .ok_or_else(|| AppError::Unauthorized("user_id required".into()))?;

    let conversation_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let orchestrator = &state.orchestrator;

    // Run the orchestrator in a background task
    let result = orchestrator
        .run_authenticated(&req.message, &conversation_id, &user_id)
        .await?;

    // Create SSE stream from the result
    let events = vec![
        Ok(Event::default().data(
            serde_json::to_string(&SseEvent::TextDelta {
                delta: result.content.clone(),
            })
            .unwrap_or_default(),
        )),
        Ok(Event::default().data(
            serde_json::to_string(&SseEvent::Done {
                content: result.content,
                conversation_id,
                turns: result.turns,
            })
            .unwrap_or_default(),
        )),
    ];

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        Box::pin(stream::iter(events));

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// POST /api/banking/chat/public — public chat endpoint (SSE streaming).
pub async fn chat_public(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>, AppError> {
    let conversation_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let orchestrator = &state.orchestrator;

    let result = orchestrator
        .run_public(&req.message, &conversation_id)
        .await?;

    let events = vec![
        Ok(Event::default().data(
            serde_json::to_string(&SseEvent::TextDelta {
                delta: result.content.clone(),
            })
            .unwrap_or_default(),
        )),
        Ok(Event::default().data(
            serde_json::to_string(&SseEvent::Done {
                content: result.content,
                conversation_id,
                turns: result.turns,
            })
            .unwrap_or_default(),
        )),
    ];

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        Box::pin(stream::iter(events));

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// POST /api/banking/chat/sync — synchronous chat endpoint (non-streaming).
pub async fn chat_sync(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    let conversation_id = req
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let user_id = req.user_id.clone();

    let result = if let Some(uid) = user_id {
        state
            .orchestrator
            .run_authenticated(&req.message, &conversation_id, &uid)
            .await?
    } else {
        state
            .orchestrator
            .run_public(&req.message, &conversation_id)
            .await?
    };

    // Determine the agent name from the active agent store
    let agent_name = state
        .active_agent_store
        .get_active_agent(&conversation_id)
        .await
        .unwrap_or_else(|| "unknown".into());

    Ok(Json(ChatResponse {
        response: result.content,
        conversation_id,
        agent_name,
        turns: result.turns,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_validation() {
        let req = ChatRequest {
            message: "Hello".into(),
            conversation_id: Some("conv-1".into()),
            user_id: Some("user-123".into()),
        };
        assert!(!req.message.is_empty());
    }
}
