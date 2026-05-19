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

/// A bank account with embedded transactions — returned by GET /api/banking/accounts/{user_id}.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankAccountDetail {
    pub user_id: String,
    pub name: String,
    pub account_id: String,
    pub balance: f64,
    pub avatar_color: String,
    pub transactions: Vec<TransactionView>,
}

/// A transaction from the perspective of a specific user (Python-compatible shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionView {
    pub tx_id: String,
    /// "debit" if the user sent funds, "credit" if received.
    #[serde(rename = "type")]
    pub tx_type: String,
    pub counterparty_id: String,
    pub counterparty_name: String,
    pub amount: f64,
    pub description: String,
    pub timestamp: String,
}

/// A transaction record between two users (internal storage).
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

impl Transaction {
    /// Convert to a user-perspective view (debit/credit, counterparty).
    pub fn to_view(&self, user_id: &str) -> TransactionView {
        let is_debit = self.from_user == user_id;
        TransactionView {
            tx_id: self.tx_id.clone(),
            tx_type: if is_debit {
                "debit".into()
            } else {
                "credit".into()
            },
            counterparty_id: if is_debit {
                self.to_user.clone()
            } else {
                self.from_user.clone()
            },
            counterparty_name: if is_debit {
                self.to_name.clone()
            } else {
                self.from_name.clone()
            },
            amount: self.amount,
            description: self.description.clone(),
            timestamp: self.timestamp.clone(),
        }
    }
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
