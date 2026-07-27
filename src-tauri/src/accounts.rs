use scrat_application::account_service::{AccountService, AccountWithBalance, ApplicationError};
use scrat_domain::account::{AccountId, AccountStatus};
use scrat_domain::money::Currency;
use scrat_infra_sqlite::{Connection, SqliteAccountRepository};
use serde::Serialize;
use tauri::State;

use crate::db::DbState;

#[derive(Debug, Serialize)]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub status: String,
    pub opening_balance_minor_units: i64,
    pub balance_minor_units: i64,
    pub currency: String,
    pub source_patterns: Vec<String>,
}

impl From<AccountWithBalance> for AccountDto {
    fn from(value: AccountWithBalance) -> Self {
        let AccountWithBalance { account, balance } = value;
        Self {
            id: account.id().as_string(),
            name: account.name().as_str().to_string(),
            status: match account.status() {
                AccountStatus::Active => "active",
                AccountStatus::Archived => "archived",
            }
            .to_string(),
            opening_balance_minor_units: account.opening_balance().minor_units(),
            balance_minor_units: balance.minor_units(),
            currency: balance.currency().code().to_string(),
            source_patterns: account
                .source_patterns()
                .iter()
                .map(|p| p.as_str().to_string())
                .collect(),
        }
    }
}

/// The app-wide currency (Settings > Set currency lands in M7); until then,
/// falls back to USD if nothing has been configured yet.
fn app_currency(conn: &Connection) -> Currency {
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'currency_code'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|code| Currency::new(&code).ok())
    .unwrap_or_else(|| Currency::new("USD").expect("USD is a valid currency code"))
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

#[tauri::command]
pub fn list_accounts(state: State<DbState>) -> Result<Vec<AccountDto>, String> {
    with_service(&state, |s| s.list_accounts_with_balance())
        .map(|accounts| accounts.into_iter().map(AccountDto::from).collect())
}

#[tauri::command]
pub fn create_account(
    state: State<DbState>,
    name: String,
    opening_balance_minor_units: i64,
) -> Result<AccountDto, String> {
    with_service(&state, |s| {
        let account = s.create_account(&name, opening_balance_minor_units)?;
        let balance = account.opening_balance().clone();
        Ok(AccountWithBalance { account, balance })
    })
    .map(AccountDto::from)
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
pub fn add_source_pattern(
    state: State<DbState>,
    id: String,
    pattern: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.add_source_pattern(id, &pattern))
}

#[tauri::command]
pub fn remove_source_pattern(
    state: State<DbState>,
    id: String,
    pattern: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.remove_source_pattern(id, &pattern))
}

#[tauri::command]
pub fn archive_account(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.archive_account(id))
}

#[tauri::command]
pub fn activate_account(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.activate_account(id))
}

#[tauri::command]
pub fn delete_account(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.delete_account(id))
}
