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
    pub balance_minor_units: i64,
    /// Whether the account's starting point has been established. When
    /// false, `balance_minor_units` is the ledger sum alone and is only
    /// correct if the account happened to begin at zero — the UI says so
    /// rather than presenting a guess as fact.
    pub is_opening_balance_set: bool,
    /// The anchor itself, so the UI can both show what it currently is (a
    /// mistyped one is otherwise invisible and therefore uncorrectable) and
    /// derive the ledger sum as `balance - opening` to preview what a new
    /// anchor would work out to. Zero when unset, which is also what it
    /// contributes to the balance — read it alongside
    /// `is_opening_balance_set`, never on its own.
    pub opening_balance_minor_units: i64,
    /// Paired with `is_opening_balance_set` to decide whether to prompt: an
    /// account with no transactions has nothing to anchor yet.
    pub has_transactions: bool,
    pub currency: String,
    pub description_patterns: Vec<String>,
    /// Whether this is the app-wide default account — used as the CSV
    /// import destination when none is explicitly chosen. Changeable via
    /// `set_default_account`.
    pub is_default: bool,
}

fn to_dto(value: AccountWithBalance, default_account_id: Option<AccountId>) -> AccountDto {
    let AccountWithBalance {
        account,
        balance,
        transaction_count,
    } = value;
    AccountDto {
        id: account.id().as_string(),
        name: account.name().as_str().to_string(),
        balance_minor_units: balance.minor_units(),
        is_opening_balance_set: account.is_opening_balance_set(),
        opening_balance_minor_units: account.opening_balance_minor_units(),
        has_transactions: transaction_count > 0,
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
        && let Ok(id) = AccountId::parse(&id_str)
        && accounts.iter().any(|a| a.id() == id)
    {
        return Ok(Some(id));
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
pub fn create_account(state: State<DbState>, name: String) -> Result<AccountDto, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let repo = SqliteAccountRepository::new(conn, currency.clone());
    let service = AccountService::new(&repo, currency.clone());
    let account = service.create_account(&name).map_err(|e| e.to_string())?;
    // A brand-new account has no transactions and no starting point, so its
    // balance is zero — the one moment where that's a fact and not a guess.
    let balance = scrat_domain::money::Money::from_minor_units(0, currency);
    let default_account_id = resolve_default_account_id(conn)?;
    Ok(to_dto(
        AccountWithBalance {
            account,
            balance,
            transaction_count: 0,
        },
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

/// Sets the account's starting point from the balance the user reads off
/// their bank, back-solving through the ledger. Replaces the old
/// `set_opening_balance`, which asked for the anchor directly — a number
/// nobody can compute by hand once any history has been imported.
#[tauri::command]
pub fn establish_opening_balance(
    state: State<DbState>,
    id: String,
    observed_balance_minor_units: i64,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| {
        s.establish_opening_balance(id, observed_balance_minor_units)
    })
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

#[cfg(test)]
mod tests {
    use scrat_domain::account::{Account, AccountName, DescriptionPattern};
    use scrat_domain::money::Money;

    use super::*;

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn with_balance(account: Account, balance_minor_units: i64, count: u64) -> AccountWithBalance {
        AccountWithBalance {
            account,
            balance: Money::from_minor_units(balance_minor_units, eur()),
            transaction_count: count,
        }
    }

    fn anchored(name: &str, opening_minor_units: i64) -> Account {
        Account::new(
            AccountId::new(),
            AccountName::new(name).unwrap(),
            Money::from_minor_units(opening_minor_units, eur()),
        )
    }

    fn unanchored(name: &str) -> Account {
        Account::without_opening_balance(AccountId::new(), AccountName::new(name).unwrap())
    }

    #[test]
    fn an_account_dto_carries_every_field_across() {
        let account = anchored("Checking", 10_000);
        let id = account.id();

        let dto = to_dto(with_balance(account, 15_000, 3), None);

        assert_eq!(dto.id, id.as_string());
        assert_eq!(dto.name, "Checking");
        assert_eq!(dto.balance_minor_units, 15_000);
        assert_eq!(dto.opening_balance_minor_units, 10_000);
        assert!(dto.is_opening_balance_set);
        assert!(dto.has_transactions);
        assert_eq!(dto.currency, "EUR");
    }

    /// The distinction migration 0008 exists to preserve, carried all the way
    /// to the UI: "we don't know where this account started" and "the user
    /// told us it started at zero" both put 0 in `opening_balance_minor_units`,
    /// and only `is_opening_balance_set` tells them apart. Collapsing them
    /// here would make the app re-ask a question the user already answered,
    /// or stop asking one it never did.
    #[test]
    fn an_unanchored_account_is_distinguishable_from_one_anchored_at_zero() {
        let unset = to_dto(with_balance(unanchored("New"), 0, 0), None);
        let set_to_zero = to_dto(with_balance(anchored("Emptied", 0), 0, 0), None);

        assert_eq!(unset.opening_balance_minor_units, 0);
        assert_eq!(set_to_zero.opening_balance_minor_units, 0);
        assert!(!unset.is_opening_balance_set);
        assert!(set_to_zero.is_opening_balance_set);
    }

    /// `has_transactions` is the "is there anything to anchor yet" flag, not
    /// a count — a fresh account is at zero either way and shouldn't prompt.
    #[test]
    fn has_transactions_is_false_only_for_an_empty_ledger() {
        assert!(!to_dto(with_balance(unanchored("New"), 0, 0), None).has_transactions);
        assert!(to_dto(with_balance(unanchored("New"), 0, 1), None).has_transactions);
    }

    #[test]
    fn the_dto_reports_the_currency_the_balance_is_denominated_in() {
        let dto = to_dto(
            AccountWithBalance {
                account: unanchored("Checking"),
                balance: Money::from_minor_units(0, Currency::new("USD").unwrap()),
                transaction_count: 0,
            },
            None,
        );

        assert_eq!(dto.currency, "USD");
    }

    #[test]
    fn the_named_default_account_is_the_only_one_flagged() {
        let chosen = unanchored("Checking");
        let other = unanchored("Savings");
        let chosen_id = chosen.id();

        let chosen_dto = to_dto(with_balance(chosen, 0, 0), Some(chosen_id));
        let other_dto = to_dto(with_balance(other, 0, 0), Some(chosen_id));

        assert!(chosen_dto.is_default);
        assert!(!other_dto.is_default);
    }

    /// With no default configured, no account may claim the flag — an
    /// `Option` compared against `Some(id)` must not treat `None` as a match.
    #[test]
    fn no_account_is_default_when_none_is_configured() {
        let dto = to_dto(with_balance(unanchored("Checking"), 0, 0), None);

        assert!(!dto.is_default);
    }

    /// Patterns reach the UI in their normalized (lowercased, trimmed) stored
    /// form and in the order they were added — that's the list the user edits.
    #[test]
    fn description_patterns_cross_the_wire_in_stored_order() {
        let mut account = unanchored("Checking");
        account.add_description_pattern(DescriptionPattern::new("  Whole FOODS ").unwrap());
        account.add_description_pattern(DescriptionPattern::new("Trader Joes").unwrap());

        let dto = to_dto(with_balance(account, 0, 0), None);

        assert_eq!(dto.description_patterns, ["whole foods", "trader joes"]);
    }

    #[test]
    fn an_account_with_no_patterns_reports_an_empty_list() {
        let dto = to_dto(with_balance(unanchored("Checking"), 0, 0), None);

        assert!(dto.description_patterns.is_empty());
    }

    /// A negative balance is a real state (an overdrawn account) and must
    /// survive intact rather than being clamped or made absolute.
    #[test]
    fn an_overdrawn_balance_keeps_its_sign() {
        let dto = to_dto(with_balance(anchored("Checking", 0), -4_250, 2), None);

        assert_eq!(dto.balance_minor_units, -4_250);
    }
}
