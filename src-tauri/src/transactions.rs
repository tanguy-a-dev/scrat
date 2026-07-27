use chrono::NaiveDate;
use scrat_application::transaction_service::TransactionService;
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::transaction::{Transaction, TransactionId};
use scrat_infra_sqlite::{
    SqliteAccountRepository, SqliteCategoryRepository, SqliteTransactionRepository,
};
use serde::Serialize;
use tauri::State;

use crate::accounts::app_currency;
use crate::db::DbState;

#[derive(Debug, Serialize)]
pub struct TransactionDto {
    pub id: String,
    pub date: String,
    pub amount_minor_units: i64,
    pub currency: String,
    pub source: String,
    pub category_id: String,
    pub account_id: String,
}

impl From<Transaction> for TransactionDto {
    fn from(transaction: Transaction) -> Self {
        Self {
            id: transaction.id().as_string(),
            date: transaction.date().format("%Y-%m-%d").to_string(),
            amount_minor_units: transaction.amount().minor_units(),
            currency: transaction.amount().currency().code().to_string(),
            source: transaction.source().as_str().to_string(),
            category_id: transaction.category_id().as_string(),
            account_id: transaction.account_id().as_string(),
        }
    }
}

pub(crate) fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|e| e.to_string())
}

pub(crate) fn with_service<T>(
    state: &State<DbState>,
    f: impl FnOnce(
        &TransactionService,
    ) -> Result<T, scrat_application::transaction_service::ApplicationError>,
) -> Result<T, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let transactions = SqliteTransactionRepository::new(conn, currency.clone());
    let accounts = SqliteAccountRepository::new(conn, currency.clone());
    let categories = SqliteCategoryRepository::new(conn);
    let service = TransactionService::new(&transactions, &accounts, &categories, currency);
    f(&service).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_transactions(
    state: State<DbState>,
    start: String,
    end: String,
) -> Result<Vec<TransactionDto>, String> {
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
    with_service(&state, |s| s.list_in_range(start, end))
        .map(|txs| txs.into_iter().map(TransactionDto::from).collect())
}

#[tauri::command]
pub fn create_transaction(
    state: State<DbState>,
    date: String,
    amount_minor_units: i64,
    source: String,
    category_id: String,
    account_id: String,
) -> Result<TransactionDto, String> {
    let date = parse_date(&date)?;
    let category_id = CategoryId::parse(&category_id).map_err(|e| e.to_string())?;
    let account_id = AccountId::parse(&account_id).map_err(|e| e.to_string())?;
    with_service(&state, |s| {
        s.create_transaction(date, amount_minor_units, &source, category_id, account_id)
    })
    .map(TransactionDto::from)
}

#[tauri::command]
pub fn delete_transaction(state: State<DbState>, id: String) -> Result<(), String> {
    let id = TransactionId::parse(&id).map_err(|e| e.to_string())?;
    with_service(&state, |s| s.delete_transaction(id))
}

#[tauri::command]
pub fn suggest_account_for_source(
    state: State<DbState>,
    source: String,
) -> Result<Option<String>, String> {
    with_service(&state, |s| s.find_account_by_source(&source))
        .map(|found| found.map(|id| id.as_string()))
}

#[tauri::command]
pub fn suggest_category_for_source(
    state: State<DbState>,
    source: String,
) -> Result<Option<String>, String> {
    with_service(&state, |s| s.suggest_category_for_source(&source))
        .map(|found| found.map(|id| id.as_string()))
}
