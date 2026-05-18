use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use crate::signals::bus::AppSignalBus;
use injectable::prelude::*;

/// A pending approval entry with a oneshot channel for response.
pub struct PendingApproval {
    /// The user who needs to approve.
    pub user_id: String,
    /// The type of action requiring approval.
    pub action_type: String,
    /// Details of the action.
    pub details: Value,
    /// When the approval was created.
    pub created_at: std::time::Instant,
    /// Sender to respond with approval decision.
    pub responder: oneshot::Sender<bool>,
}

/// Service managing human-in-the-loop approvals.
pub struct ApprovalService {
    pending: Arc<Mutex<HashMap<String, PendingApproval>>>,
    signal_bus: Arc<AppSignalBus>,
    /// Track approved transfers for one-shot token validation.
    approved_transfers: Arc<Mutex<HashMap<String, Value>>>,
}

#[injectable]
impl ApprovalService {
    /// Create a new ApprovalService.
    #[injectable(ctor)]
    pub fn new(#[injectable(inject)] signal_bus: Arc<AppSignalBus>) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            signal_bus,
            approved_transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a pending approval entry.
    pub async fn create_pending_approval(
        &self,
        conversation_id: &str,
        user_id: &str,
        action_type: &str,
        details: Value,
        responder: oneshot::Sender<bool>,
    ) {
        self.pending.lock().await.insert(
            conversation_id.to_string(),
            PendingApproval {
                user_id: user_id.to_string(),
                action_type: action_type.to_string(),
                details,
                created_at: std::time::Instant::now(),
                responder,
            },
        );
    }

    /// Respond to a pending approval (accept or reject).
    pub async fn respond(&self, conversation_id: &str, approved: bool) -> Result<(), String> {
        let mut pending = self.pending.lock().await;
        if let Some(mut entry) = pending.remove(conversation_id) {
            if entry.created_at.elapsed().as_secs() > 60 {
                return Err("Approval request expired".into());
            }
            entry
                .responder
                .send(approved)
                .map_err(|_| "Failed to send approval response".to_string())
        } else {
            Err("No pending approval found".into())
        }
    }

    /// Cancel a pending approval.
    pub async fn cancel_approval(&self, conversation_id: &str) {
        self.pending.lock().await.remove(conversation_id);
    }

    /// Check if there's a pending approval for a conversation.
    pub async fn has_pending(&self, conversation_id: &str) -> bool {
        self.pending.lock().await.contains_key(conversation_id)
    }

    /// Mark a transfer as approved (stores approval token for one-shot validation).
    pub async fn mark_approved(&self, conversation_id: &str, details: &Value) {
        let to_user = details
            .get("to_user")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let amount = details
            .get("amount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        self.approved_transfers.lock().await.insert(
            conversation_id.to_string(),
            serde_json::json!({
                "to_user": to_user,
                "amount": amount,
                "consumed": false
            }),
        );
    }

    /// Get and consume an approved transfer token.
    pub async fn get_approved_transfer(&self, conversation_id: &str) -> Option<Value> {
        let mut transfers = self.approved_transfers.lock().await;
        transfers.remove(conversation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_respond_approved() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let (tx, rx) = oneshot::channel();

        service
            .create_pending_approval("conv1", "alice", "transfer", serde_json::json!({}), tx)
            .await;

        assert!(service.has_pending("conv1").await);

        service.respond("conv1", true).await.unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx).await;
        assert!(result.unwrap().unwrap());
    }

    #[tokio::test]
    async fn test_create_and_respond_denied() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let (tx, rx) = oneshot::channel();

        service
            .create_pending_approval("conv1", "alice", "transfer", serde_json::json!({}), tx)
            .await;

        service.respond("conv1", false).await.unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx).await;
        assert!(!result.unwrap().unwrap());
    }

    #[tokio::test]
    async fn test_respond_nonexistent() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let result = service.respond("nonexistent", true).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No pending"));
    }

    #[tokio::test]
    async fn test_cancel_approval() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let (tx, _) = oneshot::channel();

        service
            .create_pending_approval("conv1", "alice", "transfer", serde_json::json!({}), tx)
            .await;

        service.cancel_approval("conv1").await;
        assert!(!service.has_pending("conv1").await);
    }

    #[tokio::test]
    async fn test_mark_approved_transfer() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let details = serde_json::json!({"to_user": "bob", "amount": 100});

        service.mark_approved("conv1", &details).await;

        let transfer = service.get_approved_transfer("conv1").await.unwrap();
        assert_eq!(transfer.get("to_user").unwrap().as_str().unwrap(), "bob");
        assert_eq!(transfer.get("amount").unwrap().as_f64().unwrap(), 100.0);
    }

    #[tokio::test]
    async fn test_get_approved_transfer_consumed() {
        let service = ApprovalService::new(Arc::new(AppSignalBus::new()));
        let details = serde_json::json!({"to_user": "bob", "amount": 100});

        service.mark_approved("conv1", &details).await;
        let _ = service.get_approved_transfer("conv1").await;

        // Second retrieval should return None (consumed)
        assert!(service.get_approved_transfer("conv1").await.is_none());
    }
}
