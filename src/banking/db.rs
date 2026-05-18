use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::banking::models::{BankAccount, Transaction};
use injectable::prelude::*;

/// In-memory bank database with demo accounts.
#[derive(Debug, Clone)]
pub struct BankDatabase {
    accounts: Arc<Mutex<HashMap<String, BankAccount>>>,
    transactions: Arc<Mutex<Vec<Transaction>>>,
}

#[injectable]
impl BankDatabase {
    /// Create a new BankDatabase with demo accounts.
    #[injectable(ctor)]
    pub fn new() -> Self {
        let mut accounts = HashMap::new();
        accounts.insert(
            "alice".into(),
            BankAccount {
                user_id: "alice".into(),
                name: "Alice".into(),
                account_id: "ACC001".into(),
                balance: 5000.0,
                avatar_color: "#4CAF50".into(),
            },
        );
        accounts.insert(
            "bob".into(),
            BankAccount {
                user_id: "bob".into(),
                name: "Bob".into(),
                account_id: "ACC002".into(),
                balance: 3200.0,
                avatar_color: "#2196F3".into(),
            },
        );
        accounts.insert(
            "charlie".into(),
            BankAccount {
                user_id: "charlie".into(),
                name: "Charlie".into(),
                account_id: "ACC003".into(),
                balance: 1800.0,
                avatar_color: "#FF9800".into(),
            },
        );
        Self {
            accounts: Arc::new(Mutex::new(accounts)),
            transactions: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Get the balance for a user.
    pub async fn get_balance(&self, user_id: &str) -> Option<f64> {
        let accounts = self.accounts.lock().await;
        accounts.get(user_id).map(|a| a.balance)
    }

    /// Transfer funds from one user to another.
    pub async fn transfer(
        &self,
        from_user: &str,
        to_user: &str,
        amount: f64,
    ) -> Result<Transaction, String> {
        if amount <= 0.0 {
            return Err("Transfer amount must be positive".into());
        }

        let mut accounts = self.accounts.lock().await;
        let from = accounts
            .get(from_user)
            .ok_or_else(|| format!("User {} not found", from_user))?;
        let to = accounts
            .get(to_user)
            .ok_or_else(|| format!("User {} not found", to_user))?;

        if from.balance < amount {
            return Err("Insufficient funds".into());
        }

        let from_name = from.name.clone();
        let to_name = to.name.clone();

        // Perform transfer
        accounts.get_mut(from_user).unwrap().balance -= amount;
        accounts.get_mut(to_user).unwrap().balance += amount;
        drop(accounts);

        let tx = Transaction {
            tx_id: format!("TX-{}", uuid::Uuid::new_v4()),
            from_user: from_user.to_string(),
            to_user: to_user.to_string(),
            amount,
            timestamp: chrono::Utc::now().to_rfc3339(),
            description: format!("Transfer from {} to {}", from_name, to_name),
            from_name,
            to_name,
        };

        let mut transactions = self.transactions.lock().await;
        transactions.push(tx.clone());

        Ok(tx)
    }

    /// Get transactions for a user (both sent and received).
    pub async fn get_transactions(&self, user_id: &str) -> Vec<Transaction> {
        let transactions = self.transactions.lock().await;
        transactions
            .iter()
            .filter(|tx| tx.from_user == user_id || tx.to_user == user_id)
            .cloned()
            .collect()
    }

    /// List all accounts.
    pub async fn list_accounts(&self) -> Vec<BankAccount> {
        let accounts = self.accounts.lock().await;
        accounts.values().cloned().collect()
    }

    /// Get a specific account by user_id.
    pub async fn get_account(&self, user_id: &str) -> Option<BankAccount> {
        let accounts = self.accounts.lock().await;
        accounts.get(user_id).cloned()
    }
}

impl Default for BankDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_database_has_three_accounts() {
        let db = BankDatabase::new();
        let accounts = db.list_accounts().await;
        assert_eq!(accounts.len(), 3);
    }

    #[tokio::test]
    async fn test_get_balance() {
        let db = BankDatabase::new();
        assert_eq!(db.get_balance("alice").await, Some(5000.0));
        assert_eq!(db.get_balance("bob").await, Some(3200.0));
        assert_eq!(db.get_balance("charlie").await, Some(1800.0));
        assert_eq!(db.get_balance("unknown").await, None);
    }

    #[tokio::test]
    async fn test_get_account() {
        let db = BankDatabase::new();
        let account = db.get_account("alice").await.unwrap();
        assert_eq!(account.user_id, "alice");
        assert_eq!(account.name, "Alice");
        assert_eq!(account.balance, 5000.0);
        assert!(db.get_account("unknown").await.is_none());
    }

    #[tokio::test]
    async fn test_transfer_success() {
        let db = BankDatabase::new();
        let tx = db.transfer("alice", "bob", 500.0).await.unwrap();
        assert_eq!(tx.from_user, "alice");
        assert_eq!(tx.to_user, "bob");
        assert_eq!(tx.amount, 500.0);
        assert_eq!(db.get_balance("alice").await, Some(4500.0));
        assert_eq!(db.get_balance("bob").await, Some(3700.0));
    }

    #[tokio::test]
    async fn test_transfer_insufficient_funds() {
        let db = BankDatabase::new();
        let result = db.transfer("charlie", "alice", 9999.0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient funds"));
    }

    #[tokio::test]
    async fn test_transfer_user_not_found() {
        let db = BankDatabase::new();
        let result = db.transfer("unknown", "alice", 100.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transfer_negative_amount() {
        let db = BankDatabase::new();
        let result = db.transfer("alice", "bob", -100.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transfer_zero_amount() {
        let db = BankDatabase::new();
        let result = db.transfer("alice", "bob", 0.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_transactions_empty() {
        let db = BankDatabase::new();
        let txs = db.get_transactions("alice").await;
        assert!(txs.is_empty());
    }

    #[tokio::test]
    async fn test_get_transactions_after_transfer() {
        let db = BankDatabase::new();
        db.transfer("alice", "bob", 100.0).await.unwrap();
        let alice_txs = db.get_transactions("alice").await;
        let bob_txs = db.get_transactions("bob").await;
        assert_eq!(alice_txs.len(), 1);
        assert_eq!(bob_txs.len(), 1);
    }

    #[tokio::test]
    async fn test_default() {
        let db = BankDatabase::default();
        let accounts = db.list_accounts().await;
        assert_eq!(accounts.len(), 3);
    }

    #[tokio::test]
    async fn test_multiple_transfers() {
        let db = BankDatabase::new();
        db.transfer("alice", "bob", 100.0).await.unwrap();
        db.transfer("bob", "charlie", 50.0).await.unwrap();
        assert_eq!(db.get_balance("alice").await, Some(4900.0));
        assert_eq!(db.get_balance("bob").await, Some(3250.0));
        assert_eq!(db.get_balance("charlie").await, Some(1850.0));
    }
}
