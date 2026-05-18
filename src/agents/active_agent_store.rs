use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use injectable::prelude::*;

/// In-memory store tracking which agent is active for each conversation.
#[derive(Debug, Clone)]
pub struct ActiveAgentStore {
    agents: Arc<Mutex<HashMap<String, String>>>,
    pending_summaries: Arc<Mutex<HashMap<String, String>>>,
}

#[injectable]
impl ActiveAgentStore {
    /// Create a new empty ActiveAgentStore.
    #[injectable(ctor)]
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            pending_summaries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the active agent for a conversation.
    pub async fn get_active_agent(&self, conversation_id: &str) -> Option<String> {
        self.agents.lock().await.get(conversation_id).cloned()
    }

    /// Set the active agent for a conversation.
    pub async fn set_active_agent(&self, conversation_id: &str, agent_name: &str) {
        self.agents
            .lock()
            .await
            .insert(conversation_id.to_string(), agent_name.to_string());
    }

    /// Get the default agent name based on authentication status.
    pub fn get_default_agent(is_authenticated: bool) -> &'static str {
        if is_authenticated {
            "AuthenticatedCRM"
        } else {
            "UnauthenticatedCRM"
        }
    }

    /// Get and clear a pending handoff summary.
    pub async fn get_and_clear_summary(&self, conversation_id: &str) -> Option<String> {
        self.pending_summaries.lock().await.remove(conversation_id)
    }

    /// Set a pending handoff summary.
    pub async fn set_pending_summary(&self, conversation_id: &str, summary: &str) {
        self.pending_summaries
            .lock()
            .await
            .insert(conversation_id.to_string(), summary.to_string());
    }

    /// Clear the active agent for a conversation.
    pub async fn clear_agent(&self, conversation_id: &str) {
        self.agents.lock().await.remove(conversation_id);
    }

    /// Check if a handoff occurred by comparing current agent with expected.
    pub async fn check_handoff(&self, conversation_id: &str, previous_agent: &str) -> bool {
        match self.get_active_agent(conversation_id).await {
            Some(current) => current != previous_agent,
            None => false,
        }
    }
}

impl Default for ActiveAgentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_active_agent() {
        let store = ActiveAgentStore::new();
        store.set_active_agent("conv1", "AuthenticatedCRM").await;
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("AuthenticatedCRM".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_nonexistent_agent() {
        let store = ActiveAgentStore::new();
        assert_eq!(store.get_active_agent("nonexistent").await, None);
    }

    #[tokio::test]
    async fn test_clear_agent() {
        let store = ActiveAgentStore::new();
        store.set_active_agent("conv1", "AuthenticatedCRM").await;
        store.clear_agent("conv1").await;
        assert_eq!(store.get_active_agent("conv1").await, None);
    }

    #[tokio::test]
    async fn test_pending_summary() {
        let store = ActiveAgentStore::new();
        store
            .set_pending_summary("conv1", "User wants to transfer funds")
            .await;
        let summary = store.get_and_clear_summary("conv1").await;
        assert_eq!(summary, Some("User wants to transfer funds".to_string()));
        // Summary should be cleared after retrieval
        assert_eq!(store.get_and_clear_summary("conv1").await, None);
    }

    #[tokio::test]
    async fn test_get_default_agent() {
        assert_eq!(
            ActiveAgentStore::get_default_agent(true),
            "AuthenticatedCRM"
        );
        assert_eq!(
            ActiveAgentStore::get_default_agent(false),
            "UnauthenticatedCRM"
        );
    }

    #[tokio::test]
    async fn test_check_handoff() {
        let store = ActiveAgentStore::new();
        store.set_active_agent("conv1", "AuthenticatedCRM").await;
        assert!(!store.check_handoff("conv1", "AuthenticatedCRM").await);
        assert!(store.check_handoff("conv1", "UnauthenticatedCRM").await);
    }

    #[tokio::test]
    async fn test_check_handoff_no_agent() {
        let store = ActiveAgentStore::new();
        assert!(!store.check_handoff("conv1", "UnauthenticatedCRM").await);
    }

    #[tokio::test]
    async fn test_default() {
        let store = ActiveAgentStore::default();
        assert_eq!(store.get_active_agent("conv1").await, None);
    }

    #[tokio::test]
    async fn test_overwrite_active_agent() {
        let store = ActiveAgentStore::new();
        store.set_active_agent("conv1", "AuthenticatedCRM").await;
        store.set_active_agent("conv1", "BankTransfer").await;
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("BankTransfer".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiple_conversations() {
        let store = ActiveAgentStore::new();
        store.set_active_agent("conv1", "AuthenticatedCRM").await;
        store.set_active_agent("conv2", "BankTransfer").await;
        assert_eq!(
            store.get_active_agent("conv1").await,
            Some("AuthenticatedCRM".to_string())
        );
        assert_eq!(
            store.get_active_agent("conv2").await,
            Some("BankTransfer".to_string())
        );
    }
}
