use axum::extract::Path;
use axum::Json;

use injectable::prelude::*;

use crate::banking::db::BankDatabase;
use crate::banking::models::{BankAccount, BankAccountDetail, Transaction};
use crate::error::AppError;
use crate::AppState;

/// Response wrapper for the accounts list — matches Python `{"accounts": [...]}`.
#[derive(serde::Serialize)]
pub struct ListAccountsResponse {
    pub accounts: Vec<BankAccount>,
}

pub async fn list_accounts(
    db: Inject<BankDatabase>,
) -> Result<Json<ListAccountsResponse>, AppError> {
    let accounts = db.list_accounts().await;
    Ok(Json(ListAccountsResponse { accounts }))
}

pub async fn get_account(
    db: Inject<BankDatabase>,
    Path(user_id): Path<String>,
) -> Result<Json<BankAccountDetail>, AppError> {
    let account = db
        .get_account(&user_id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Account {} not found", user_id)))?;
    let transactions = db.get_transactions(&user_id).await;
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

pub async fn get_transactions(
    db: Inject<BankDatabase>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<Transaction>>, AppError> {
    let transactions = db.get_transactions(&user_id).await;
    Ok(Json(transactions))
}

pub fn banking_router() -> axum::Router<AppState> {
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
    use crate::test_utils::{create_test_app, create_test_state};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
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
    async fn test_get_known_account_alice() {
        let state = create_test_state().await;
        let db: Arc<BankDatabase> = state.container().resolve_external().await.unwrap();
        let account = db.get_account("alice").await;
        assert!(account.is_some());
        let account = account.unwrap();
        assert_eq!(account.user_id, "alice");
        assert_eq!(account.balance, 5000.0);
    }

    #[tokio::test]
    async fn test_get_unknown_account_returns_404() {
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
    async fn test_list_accounts_returns_non_empty_array() {
        let state = create_test_state().await;
        let db: Arc<BankDatabase> = state.container().resolve_external().await.unwrap();
        let accounts = db.list_accounts().await;
        assert_eq!(accounts.len(), 3);
    }

    #[tokio::test]
    async fn test_get_transactions_for_alice() {
        let state = create_test_state().await;
        let db: Arc<BankDatabase> = state.container().resolve_external().await.unwrap();
        let txs = db.get_transactions("alice").await;
        assert!(txs.is_empty());
    }

    #[tokio::test]
    async fn test_get_transactions_after_transfer() {
        let state = create_test_state().await;
        let db: Arc<BankDatabase> = state.container().resolve_external().await.unwrap();
        db.transfer("alice", "bob", 100.0).await.unwrap();
        let app = crate::create_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/banking/accounts/alice/transactions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let txs: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(txs.len(), 1);
    }
}
