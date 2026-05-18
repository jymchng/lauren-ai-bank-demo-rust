use serde::{Deserialize, Serialize};

/// A bank account with balance and user information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankAccount {
    pub user_id: String,
    pub name: String,
    pub account_id: String,
    pub balance: f64,
    pub avatar_color: String,
}

/// A transaction record between two users.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    pub tx_id: String,
    pub from_user: String,
    pub to_user: String,
    pub amount: f64,
    pub timestamp: String,
    pub description: String,
    pub from_name: String,
    pub to_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bank_account_serialization() {
        let account = BankAccount {
            user_id: "alice".into(),
            name: "Alice".into(),
            account_id: "ACC001".into(),
            balance: 5000.0,
            avatar_color: "#4CAF50".into(),
        };
        let json = serde_json::to_string(&account).unwrap();
        let deserialized: BankAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(account, deserialized);
    }

    #[test]
    fn test_transaction_serialization() {
        let tx = Transaction {
            tx_id: "TX001".into(),
            from_user: "alice".into(),
            to_user: "bob".into(),
            amount: 100.0,
            timestamp: "2024-01-01T00:00:00Z".into(),
            description: "Test transfer".into(),
            from_name: "Alice".into(),
            to_name: "Bob".into(),
        };
        let json = serde_json::to_string(&tx).unwrap();
        let deserialized: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, deserialized);
    }
}
