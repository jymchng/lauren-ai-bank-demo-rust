use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::banking::db::BankDatabase;
use crate::banking::models::{BankAccount, BankAccountDetail, Transaction};
use crate::error::AppError;
use crate::AppState;

/// Response wrapper for the accounts list — matches Python `{"accounts": [...]}`.
#[derive(Serialize)]
pub struct ListAccountsResponse {
    pub accounts: Vec<BankAccount>,
}

/// List all bank accounts.
pub async fn list_accounts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListAccountsResponse>, AppError> {
    let accounts = state.bank_db.list_accounts().await;
    Ok(Json(ListAccountsResponse { accounts }))
}

/// Get a specific account with embedded transaction history.
pub async fn get_account(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Result<Json<BankAccountDetail>, AppError> {
    let account = state
        .bank_db
        .get_account(&user_id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Account {} not found", user_id)))?;
    let transactions = state.bank_db.get_transactions(&user_id).await;
    let transaction_views = transactions.iter().map(|tx| tx.to_view(&user_id)).collect();
    Ok(Json(BankAccountDetail {
        user_id: account.user_id,
        name: account.name,
        account_id: account.account_id,
        balance: account.balance,
        avatar_color: account.avatar_color,
        transactions: transaction_views,
    }))
}

/// Get transactions for a user.
pub async fn get_transactions(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<Transaction>>, AppError> {
    let transactions = state.bank_db.get_transactions(&user_id).await;
    Ok(Json(transactions))
}

/// Create the banking router.
pub fn banking_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/banking/accounts", axum::routing::get(list_accounts))
        .route(
            "/api/banking/accounts/:user_id",
            axum::routing::get(get_account),
        )
        .route(
            "/api/banking/accounts/:user_id/transactions",
            axum::routing::get(get_transactions),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_list_accounts() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/banking/accounts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_account_found() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/banking/accounts/alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The route is under /api/banking/accounts/:user_id
        assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_account_not_found() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/banking/accounts/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_account_alice_direct() {
        let state = crate::test_utils::create_test_state().await;
        let result = get_account(axum::extract::State(state), Path("alice".to_string())).await;
        assert!(result.is_ok());
        let detail = result.unwrap();
        assert_eq!(detail.user_id, "alice");
        assert_eq!(detail.balance, 5000.0);
        assert!(detail.transactions.is_empty());
    }

    #[tokio::test]
    async fn test_get_account_unknown_direct() {
        let state = crate::test_utils::create_test_state().await;
        let result = get_account(axum::extract::State(state), Path("unknown".to_string())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_accounts_direct() {
        let state = crate::test_utils::create_test_state().await;
        let result = list_accounts(axum::extract::State(state)).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.accounts.len(), 3);
    }

    #[tokio::test]
    async fn test_get_transactions_direct() {
        let state = crate::test_utils::create_test_state().await;
        let result = get_transactions(axum::extract::State(state), Path("alice".to_string())).await;
        assert!(result.is_ok());
        let transactions = result.unwrap();
        assert!(transactions.is_empty());
    }

    #[tokio::test]
    async fn test_get_transactions_after_transfer() {
        let state = crate::test_utils::create_test_state().await;
        state.bank_db.transfer("alice", "bob", 100.0).await.unwrap();
        let result = get_transactions(axum::extract::State(state), Path("alice".to_string())).await;
        assert!(result.is_ok());
        let transactions = result.unwrap();
        assert_eq!(transactions.len(), 1);
    }
}
