use scrat_application::account_service::{AccountService, AccountWithBalance, ApplicationError};
use scrat_domain::account::AccountId;
use scrat_domain::money::Currency;
use scrat_domain::ports::AccountRepository;
use scrat_infra_sqlite::{Connection, SqliteAccountRepository};
use serde::Serialize;
use tauri::State;

use crate::db::DbState;

#[derive(Debug, Serialize)]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub opening_balance_minor_units: i64,
    pub balance_minor_units: i64,
    pub currency: String,
    pub description_patterns: Vec<String>,
    /// Whether this is the app-wide default account — used as the CSV
    /// import destination when none is explicitly chosen. Changeable via
    /// `set_default_account`.
    pub is_default: bool,
}

fn to_dto(value: AccountWithBalance, default_account_id: Option<AccountId>) -> AccountDto {
    let AccountWithBalance { account, balance } = value;
    AccountDto {
        id: account.id().as_string(),
        name: account.name().as_str().to_string(),
        opening_balance_minor_units: account.opening_balance().minor_units(),
        balance_minor_units: balance.minor_units(),
        currency: balance.currency().code().to_string(),
        description_patterns: account
            .description_patterns()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        is_default: default_account_id == Some(account.id()),
    }
}

/// The app-wide currency (set via Settings > Set currency); falls back to
/// EUR if nothing has been configured yet.
pub(crate) fn app_currency(conn: &Connection) -> Currency {
    scrat_infra_sqlite::get_currency_code(conn)
        .ok()
        .flatten()
        .and_then(|code| Currency::new(&code).ok())
        .unwrap_or_else(|| Currency::new("EUR").expect("EUR is a valid currency code"))
}

fn parse_id(id: &str) -> Result<AccountId, String> {
    AccountId::parse(id).map_err(|e| e.to_string())
}

fn with_service<T>(
    state: &State<DbState>,
    f: impl FnOnce(&AccountService) -> Result<T, ApplicationError>,
) -> Result<T, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency.clone());
    let service = AccountService::new(&repo, currency);
    f(&service).map_err(|e| e.to_string())
}

/// Resolves the app-wide default account id: whatever's configured in
/// settings, as long as it still exists — otherwise, if there's exactly one
/// account, treats it as an implicit default (persisting that so future
/// reads are stable). Returns `None` when nothing can be resolved (no
/// accounts yet, or several with nothing chosen) — callers that need a hard
/// default (CSV import) must handle that case explicitly.
pub(crate) fn resolve_default_account_id(conn: &Connection) -> Result<Option<AccountId>, String> {
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency);
    let accounts = repo.list_all().map_err(|e| e.to_string())?;

    if let Some(id_str) =
        scrat_infra_sqlite::get_default_account_id(conn).map_err(|e| e.to_string())?
    {
        if let Ok(id) = AccountId::parse(&id_str) {
            if accounts.iter().any(|a| a.id() == id) {
                return Ok(Some(id));
            }
        }
    }

    let mut all = accounts.iter();
    let (Some(only), None) = (all.next(), all.next()) else {
        return Ok(None);
    };
    let id = only.id();
    scrat_infra_sqlite::set_default_account_id(conn, &id.as_string()).map_err(|e| e.to_string())?;
    Ok(Some(id))
}

#[tauri::command]
pub fn list_accounts(state: State<DbState>) -> Result<Vec<AccountDto>, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency.clone());
    let service = AccountService::new(&repo, currency);
    let accounts = service
        .list_accounts_with_balance()
        .map_err(|e| e.to_string())?;
    let default_account_id = resolve_default_account_id(conn)?;
    Ok(accounts
        .into_iter()
        .map(|a| to_dto(a, default_account_id))
        .collect())
}

#[tauri::command]
pub fn create_account(
    state: State<DbState>,
    name: String,
    opening_balance_minor_units: i64,
) -> Result<AccountDto, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency.clone());
    let service = AccountService::new(&repo, currency);
    let account = service
        .create_account(&name, opening_balance_minor_units)
        .map_err(|e| e.to_string())?;
    let balance = account.opening_balance().clone();
    let default_account_id = resolve_default_account_id(conn)?;
    Ok(to_dto(
        AccountWithBalance { account, balance },
        default_account_id,
    ))
}

#[tauri::command]
pub fn set_default_account(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency);
    repo.find_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    scrat_infra_sqlite::set_default_account_id(conn, &id.as_string()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_account(state: State<DbState>, id: String, name: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.rename_account(id, &name))
}

#[tauri::command]
pub fn set_opening_balance(
    state: State<DbState>,
    id: String,
    minor_units: i64,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.set_opening_balance(id, minor_units))
}

#[tauri::command]
pub fn add_description_pattern(
    state: State<DbState>,
    id: String,
    pattern: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.add_description_pattern(id, &pattern))
}

#[tauri::command]
pub fn remove_description_pattern(
    state: State<DbState>,
    id: String,
    pattern: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.remove_description_pattern(id, &pattern))
}

#[tauri::command]
pub fn delete_account(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.delete_account(id))
}
