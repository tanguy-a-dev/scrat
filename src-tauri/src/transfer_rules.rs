use scrat_application::transaction_service::TransactionService;
use scrat_application::transfer_rule_service::TransferRuleService;
use scrat_domain::account::AccountId;
use scrat_domain::ports::TransferRuleRepository;
use scrat_domain::transfer_rule::{TransferRule, TransferRuleId};
use scrat_infra_sqlite::{
    SqliteAccountRepository, SqliteCategoryRepository, SqliteTransactionRepository,
    SqliteTransferRuleRepository,
};
use serde::Serialize;
use tauri::State;

use crate::accounts::app_currency;
use crate::db::DbState;
use crate::errors::AppError;

#[derive(Debug, Serialize)]
pub struct TransferRuleDto {
    pub id: String,
    pub pattern: String,
    pub counterpart_account_id: String,
}

impl From<TransferRule> for TransferRuleDto {
    fn from(rule: TransferRule) -> Self {
        Self {
            id: rule.id().as_string(),
            pattern: rule.pattern().as_str().to_string(),
            counterpart_account_id: rule.counterpart_account_id().as_string(),
        }
    }
}

fn with_service<T>(
    state: &State<DbState>,
    f: impl FnOnce(
        &TransferRuleService,
    ) -> Result<T, scrat_application::transfer_rule_service::ApplicationError>,
) -> Result<T, AppError> {
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    let currency = app_currency(conn);
    let rules = SqliteTransferRuleRepository::new(conn);
    let accounts = SqliteAccountRepository::new(conn, currency);
    let service = TransferRuleService::new(&rules, &accounts);
    Ok(f(&service)?)
}

#[tauri::command]
pub fn list_transfer_rules(state: State<DbState>) -> Result<Vec<TransferRuleDto>, AppError> {
    with_service(&state, |s| s.list_rules())
        .map(|rules| rules.into_iter().map(TransferRuleDto::from).collect())
}

#[tauri::command]
pub fn create_transfer_rule(
    state: State<DbState>,
    pattern: String,
    counterpart_account_id: String,
) -> Result<TransferRuleDto, AppError> {
    let counterpart_account_id = AccountId::parse(&counterpart_account_id)?;
    with_service(&state, |s| s.create_rule(&pattern, counterpart_account_id))
        .map(TransferRuleDto::from)
}

#[tauri::command]
pub fn delete_transfer_rule(state: State<DbState>, id: String) -> Result<(), AppError> {
    let id = TransferRuleId::parse(&id)?;
    with_service(&state, |s| s.delete_rule(id))
}

#[derive(Debug, Serialize)]
pub struct ApplyTransferRulesSummaryDto {
    /// How many already-imported rows this account's incoming-transfer
    /// rules recognized and converted, mirroring each onto this account.
    pub converted: usize,
}

/// Catches up transactions imported *before* a "transfers into this
/// account" rule existed. Scoped to `account_id`: only rules whose
/// counterpart is this account are applied, matching the mental model of
/// the section this button sits under on the Accounts page.
#[tauri::command]
pub fn apply_transfer_rules(
    state: State<DbState>,
    account_id: String,
) -> Result<ApplyTransferRulesSummaryDto, AppError> {
    let account_id = AccountId::parse(&account_id)?;
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    let currency = app_currency(conn);
    let rules: Vec<TransferRule> = SqliteTransferRuleRepository::new(conn)
        .list_all()?
        .into_iter()
        .filter(|rule| rule.counterpart_account_id() == account_id)
        .collect();

    let transactions = SqliteTransactionRepository::new(conn, currency.clone());
    let accounts = SqliteAccountRepository::new(conn, currency.clone());
    let categories = SqliteCategoryRepository::new(conn);
    let service = TransactionService::new(&transactions, &accounts, &categories, currency);

    let outcome = service.apply_transfer_rules_to_existing(&rules)?;
    Ok(ApplyTransferRulesSummaryDto {
        converted: outcome.converted,
    })
}
