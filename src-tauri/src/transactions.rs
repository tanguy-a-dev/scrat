use std::collections::HashMap;

use chrono::{Local, Months, NaiveDate};
use scrat_application::transaction_service::{TransactionService, RECONCILIATION_SOURCE};
use scrat_domain::account::{Account, AccountId};
use scrat_domain::category::{Category, CategoryId};
use scrat_domain::ports::{AccountRepository, CategoryRepository};
use scrat_domain::recurring::RecurringCharge;
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
    /// `"normal"`, `"transfer"` or `"adjustment"`. Anything other than
    /// `"normal"` moves or corrects money that was already the user's, so
    /// it must be left out of income and expense totals — see
    /// `TransactionRole` in the domain layer for why.
    pub role: String,
    /// Shared by both legs of a transfer; `null` otherwise.
    pub transfer_group_id: Option<String>,
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
            role: transaction.role().as_str().to_string(),
            transfer_group_id: transaction.transfer_group_id().map(|id| id.as_string()),
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

/// Brings an account whose statements can't be imported back in line with
/// the balance the user reads off their bank's own app, by posting the
/// difference as a single adjustment entry.
///
/// Returns `null` when the ledger already agreed and nothing was written,
/// so the UI can say "already up to date" rather than claiming a correction
/// it didn't make.
#[tauri::command]
pub fn reconcile_account(
    state: State<DbState>,
    account_id: String,
    observed_balance_minor_units: i64,
    date: String,
) -> Result<Option<TransactionDto>, String> {
    let account_id = AccountId::parse(&account_id).map_err(|e| e.to_string())?;
    let date = parse_date(&date)?;
    with_service(&state, |s| {
        // Adjustments get their own category, created on first use, so they
        // are distinguishable in the ledger from anything the user filed by
        // hand — and so they don't silently pad whatever the default
        // category happens to be.
        let category_id = s.get_or_create_category_by_name(RECONCILIATION_SOURCE)?;
        s.reconcile_account(account_id, observed_balance_minor_units, category_id, date)
    })
    .map(|adjustment| adjustment.map(TransactionDto::from))
}

#[tauri::command]
pub fn set_transaction_category(
    state: State<DbState>,
    id: String,
    category_id: String,
) -> Result<(), String> {
    let id = TransactionId::parse(&id).map_err(|e| e.to_string())?;
    let category_id = CategoryId::parse(&category_id).map_err(|e| e.to_string())?;
    with_service(&state, |s| s.set_category(id, category_id))
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

#[derive(Debug, Serialize)]
pub struct RecurringChargeDto {
    pub label: String,
    /// "weekly" | "monthly" | "quarterly" | "yearly".
    pub cadence: String,
    /// Positive magnitude — a recurring charge is a cost, and carrying the
    /// sign here only invites a double negation on the way to the screen.
    pub typical_amount_minor_units: i64,
    pub monthly_equivalent_minor_units: i64,
    pub currency: String,
    pub occurrences: usize,
    pub first_seen: String,
    pub last_seen: String,
    pub next_expected: String,
    pub category_id: String,
    pub is_active: bool,
}

impl RecurringChargeDto {
    fn from_domain(charge: RecurringCharge, currency: &str) -> Self {
        Self {
            label: charge.label,
            cadence: charge.cadence.as_str().to_string(),
            typical_amount_minor_units: charge.typical_amount_minor_units,
            monthly_equivalent_minor_units: charge.monthly_equivalent_minor_units,
            currency: currency.to_string(),
            occurrences: charge.occurrences,
            first_seen: charge.first_seen.format("%Y-%m-%d").to_string(),
            last_seen: charge.last_seen.format("%Y-%m-%d").to_string(),
            next_expected: charge.next_expected.format("%Y-%m-%d").to_string(),
            category_id: charge.category_id.as_string(),
            is_active: charge.is_active,
        }
    }
}

/// How far back a recurring scan looks. Two years is enough for a yearly
/// charge to bill three times (barely) while keeping something cancelled in
/// the distant past from lingering in the list forever.
const RECURRING_LOOKBACK_MONTHS: u32 = 24;

#[tauri::command]
pub fn list_recurring_charges(state: State<DbState>) -> Result<Vec<RecurringChargeDto>, String> {
    let today = Local::now().date_naive();
    let start = today
        .checked_sub_months(Months::new(RECURRING_LOOKBACK_MONTHS))
        .unwrap_or(today);

    // Repositories built inline under one lock rather than going through
    // `with_service`, which would hand back the charges but not the currency
    // they're denominated in — and taking the lock a second time to read it
    // is how a non-reentrant Mutex turns into a deadlock.
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let transactions = SqliteTransactionRepository::new(conn, currency.clone());
    let accounts = SqliteAccountRepository::new(conn, currency.clone());
    let categories = SqliteCategoryRepository::new(conn);
    let service = TransactionService::new(&transactions, &accounts, &categories, currency.clone());

    let charges = service
        .detect_recurring_charges(start, today)
        .map_err(|e| e.to_string())?;

    Ok(charges
        .into_iter()
        .map(|charge| RecurringChargeDto::from_domain(charge, currency.code()))
        .collect())
}

/// Formats minor units as a decimal string with a comma separator (e.g.
/// `1234` -> `"12,34"`), matching both the app's own on-screen formatting
/// and the decimal-comma convention `infra-csv` already expects on import —
/// deliberately not "." so a re-import of this file's output round-trips.
fn format_amount_for_csv(minor_units: i64) -> String {
    let sign = if minor_units < 0 { "-" } else { "" };
    let abs = minor_units.unsigned_abs();
    format!("{sign}{},{:02}", abs / 100, abs % 100)
}

/// Quotes a CSV field only if it contains the delimiter, a quote, or a
/// newline — doubling any embedded quotes, per the usual CSV escaping rule.
fn csv_field(value: &str) -> String {
    if value.contains(';') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Resolves the `(Category, Subcategory)` pair a transaction exports as.
/// Categories are a strict two-level hierarchy, so a transaction filed under
/// a subcategory reports its parent under `Category` and itself under
/// `Subcategory`; one filed directly under a top-level category leaves
/// `Subcategory` empty. Splitting them into two columns (rather than emitting
/// `"Parent / Child"` in one) keeps the file pivot-table-friendly and lets a
/// re-import read the `Category` column exactly as it did before.
fn category_columns<'a>(
    id: CategoryId,
    by_id: &HashMap<CategoryId, &'a Category>,
) -> (&'a str, &'a str) {
    let Some(category) = by_id.get(&id) else {
        return ("", "");
    };
    match category.parent_id().and_then(|parent| by_id.get(&parent)) {
        Some(parent) => (parent.name().as_str(), category.name().as_str()),
        // Either a top-level category, or — defensively — a subcategory whose
        // parent is missing from the list. Either way the category itself is
        // the most specific name available, so it belongs in `Category`.
        None => (category.name().as_str(), ""),
    }
}

/// Renders the export file body. Kept separate from the Tauri command (which
/// only wires up repositories) so the formatting and hierarchy-resolution
/// rules are unit-testable without a database or a Tauri runtime.
fn build_csv(
    transactions: &[Transaction],
    accounts: &[Account],
    categories: &[Category],
) -> String {
    let account_names: HashMap<AccountId, &str> = accounts
        .iter()
        .map(|a| (a.id(), a.name().as_str()))
        .collect();
    let categories_by_id: HashMap<CategoryId, &Category> =
        categories.iter().map(|c| (c.id(), c)).collect();

    let mut csv = String::from("Date;Amount;Currency;Source;Category;Subcategory;Account\n");
    for t in transactions {
        let (category, subcategory) = category_columns(t.category_id(), &categories_by_id);
        let account = account_names
            .get(&t.account_id())
            .copied()
            .unwrap_or_default();
        csv.push_str(&format!(
            "{};{};{};{};{};{};{}\n",
            t.date().format("%Y-%m-%d"),
            format_amount_for_csv(t.amount().minor_units()),
            t.amount().currency().code(),
            csv_field(t.source().as_str()),
            csv_field(category),
            csv_field(subcategory),
            csv_field(account),
        ));
    }
    csv
}

/// Exports every transaction in the app as a semicolon-separated CSV,
/// resolving account/category ids to their display names since those (not
/// raw ids) are what makes the file useful outside Scrat.
#[tauri::command]
pub fn export_transactions_csv(state: State<DbState>, destination: String) -> Result<(), String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let currency = app_currency(conn);
    let transactions = SqliteTransactionRepository::new(conn, currency.clone());
    let accounts = SqliteAccountRepository::new(conn, currency.clone());
    let categories = SqliteCategoryRepository::new(conn);
    let service = TransactionService::new(&transactions, &accounts, &categories, currency);

    let all = service.list_all().map_err(|e| e.to_string())?;
    let all_accounts = accounts.list_all().map_err(|e| e.to_string())?;
    let all_categories = categories.list_all().map_err(|e| e.to_string())?;

    let csv = build_csv(&all, &all_accounts, &all_categories);

    std::fs::write(&destination, csv).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use scrat_domain::account::AccountName;
    use scrat_domain::category::CategoryName;
    use scrat_domain::money::{Currency, Money};
    use scrat_domain::transaction::SourceText;

    use super::*;

    const HEADER: &str = "Date;Amount;Currency;Source;Category;Subcategory;Account\n";

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn account(name: &str) -> Account {
        Account::new(
            AccountId::new(),
            AccountName::new(name).unwrap(),
            Money::zero(eur()),
        )
    }

    fn category(name: &str, parent_id: Option<CategoryId>) -> Category {
        Category::new(
            CategoryId::new(),
            CategoryName::new(name).unwrap(),
            parent_id,
        )
        .unwrap()
    }

    fn transaction(
        minor_units: i64,
        source: &str,
        category_id: CategoryId,
        account_id: AccountId,
    ) -> Transaction {
        Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(),
            Money::from_minor_units(minor_units, eur()),
            SourceText::new(source).unwrap(),
            category_id,
            account_id,
        )
        .unwrap()
    }

    /// Returns an export's data rows, asserting the header along the way.
    fn rows(csv: &str) -> Vec<&str> {
        csv.strip_prefix(HEADER)
            .expect("export must start with the documented header")
            .lines()
            .collect()
    }

    #[test]
    fn export_of_nothing_is_header_only() {
        assert_eq!(build_csv(&[], &[], &[]), HEADER);
    }

    #[test]
    fn subcategory_reports_parent_in_category_column() {
        let acc = account("Checking");
        let parent = category("Food", None);
        let child = category("Groceries", Some(parent.id()));
        let tx = transaction(-1250, "SUPERMARKET", child.id(), acc.id());

        let csv = build_csv(&[tx], &[acc], &[parent, child]);

        assert_eq!(
            rows(&csv),
            vec!["2026-03-14;-12,50;EUR;SUPERMARKET;Food;Groceries;Checking"]
        );
    }

    #[test]
    fn top_level_category_leaves_subcategory_empty() {
        let acc = account("Checking");
        let cat = category("Salary", None);
        let tx = transaction(250_000, "ACME PAYROLL", cat.id(), acc.id());

        let csv = build_csv(&[tx], &[acc], &[cat]);

        assert_eq!(
            rows(&csv),
            vec!["2026-03-14;2500,00;EUR;ACME PAYROLL;Salary;;Checking"]
        );
    }

    #[test]
    fn category_column_still_holds_the_top_level_name_after_adding_subcategory() {
        // Guards the compatibility promise in `category_columns`: whichever
        // level a transaction is filed at, column 5 is always a top-level
        // category name, so a re-import reading that column is unaffected.
        let acc = account("Checking");
        let parent = category("Food", None);
        let child = category("Groceries", Some(parent.id()));
        let deep = transaction(-500, "SHOP", child.id(), acc.id());
        let shallow = transaction(-700, "MARKET", parent.id(), acc.id());

        let csv = build_csv(&[deep, shallow], &[acc], &[parent, child]);

        let categories: Vec<&str> = rows(&csv)
            .iter()
            .map(|row| row.split(';').nth(4).unwrap())
            .collect();
        assert_eq!(categories, vec!["Food", "Food"]);
    }

    #[test]
    fn unknown_category_and_account_export_as_empty_fields() {
        // A dangling id shouldn't lose the row — the rest of it is still
        // valid ledger history.
        let tx = transaction(-100, "MYSTERY", CategoryId::new(), AccountId::new());

        let csv = build_csv(&[tx], &[], &[]);

        assert_eq!(rows(&csv), vec!["2026-03-14;-1,00;EUR;MYSTERY;;;"]);
    }

    #[test]
    fn orphaned_subcategory_falls_back_to_its_own_name() {
        let acc = account("Checking");
        let missing_parent = CategoryId::new();
        let child = category("Groceries", Some(missing_parent));
        let tx = transaction(-100, "SHOP", child.id(), acc.id());

        let csv = build_csv(&[tx], &[acc], &[child]);

        assert_eq!(
            rows(&csv),
            vec!["2026-03-14;-1,00;EUR;SHOP;Groceries;;Checking"]
        );
    }

    #[test]
    fn category_names_containing_the_delimiter_are_quoted() {
        let acc = account("Checking");
        let parent = category("Bills; utilities", None);
        let child = category("Water \"meter\"", Some(parent.id()));
        let tx = transaction(-4200, "CITY; WATER", child.id(), acc.id());

        let csv = build_csv(&[tx], &[acc], &[parent, child]);

        assert_eq!(
            rows(&csv),
            vec![
                "2026-03-14;-42,00;EUR;\"CITY; WATER\";\"Bills; utilities\";\"Water \"\"meter\"\"\";Checking"
            ]
        );
    }

    #[test]
    fn amounts_keep_two_decimals_and_a_comma_separator() {
        assert_eq!(format_amount_for_csv(0), "0,00");
        assert_eq!(format_amount_for_csv(5), "0,05");
        assert_eq!(format_amount_for_csv(-5), "-0,05");
        assert_eq!(format_amount_for_csv(-1_234), "-12,34");
        assert_eq!(format_amount_for_csv(100_000), "1000,00");
    }

    #[test]
    fn amount_formatting_survives_the_most_negative_amount() {
        // `-i64::MIN` overflows a naive `abs()`; `unsigned_abs` is why this
        // doesn't panic. Cheap regression guard, since the value is reachable
        // from a bad CSV import rather than only from a synthetic test.
        assert_eq!(format_amount_for_csv(i64::MIN), "-92233720368547758,08");
    }

    #[test]
    fn csv_field_leaves_ordinary_values_untouched() {
        assert_eq!(csv_field("Groceries"), "Groceries");
        assert_eq!(csv_field(""), "");
        assert_eq!(csv_field("Caf\u{e9}, Bar"), "Caf\u{e9}, Bar");
    }

    #[test]
    fn csv_field_quotes_embedded_newlines() {
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_field("crlf\r\nline"), "\"crlf\r\nline\"");
    }
}
