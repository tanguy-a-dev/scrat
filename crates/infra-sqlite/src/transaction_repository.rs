use chrono::NaiveDate;
use rusqlite::{params, Connection};
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{RepositoryError, TransactionRepository};
use scrat_domain::transaction::{
    SourceText, Transaction, TransactionId, TransactionRole, TransferGroupId,
};

pub struct SqliteTransactionRepository<'a> {
    conn: &'a Connection,
    currency: Currency,
}

impl<'a> SqliteTransactionRepository<'a> {
    pub fn new(conn: &'a Connection, currency: Currency) -> Self {
        Self { conn, currency }
    }

    fn row_to_transaction(&self, row: &rusqlite::Row) -> rusqlite::Result<Transaction> {
        let id_str: String = row.get("id")?;
        let id = TransactionId::parse(&id_str).map_err(invalid_column)?;
        let date_str: String = row.get("date")?;
        let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        let amount_minor_units: i64 = row.get("amount_minor_units")?;
        let source: String = row.get("source")?;
        let source = SourceText::new(&source).map_err(invalid_column)?;
        let category_id: String = row.get("category_id")?;
        let category_id = CategoryId::parse(&category_id).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        let account_id: String = row.get("account_id")?;
        let account_id = AccountId::parse(&account_id).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;

        let role: String = row.get("role")?;
        let role = TransactionRole::parse(&role).map_err(invalid_column)?;
        let transfer_group_id: Option<String> = row.get("transfer_group_id")?;
        let transfer_group_id = transfer_group_id
            .map(|raw| TransferGroupId::parse(&raw))
            .transpose()
            .map_err(invalid_column)?;

        let amount = Money::from_minor_units(amount_minor_units, self.currency.clone());
        Transaction::new_with_role(
            id,
            date,
            amount,
            source,
            category_id,
            account_id,
            role,
            transfer_group_id,
        )
        .map_err(invalid_column)
    }
}

fn invalid_column<E: std::fmt::Display>(e: E) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
}

fn sql_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError(e.to_string())
}

const INSERT_SQL: &str = "INSERT INTO transactions
    (id, date, amount_minor_units, source, category_id, account_id, dedup_key, created_at,
     role, transfer_group_id)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Every column `row_to_transaction` reads, so the three read paths can't
/// drift apart and silently drop a role or a transfer group.
const SELECT_COLUMNS: &str = "id, date, amount_minor_units, source, category_id, account_id,
     role, transfer_group_id";

impl<'a> TransactionRepository for SqliteTransactionRepository<'a> {
    fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError> {
        let date = transaction.date().format("%Y-%m-%d").to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                INSERT_SQL,
                params![
                    transaction.id().as_string(),
                    date,
                    transaction.amount().minor_units(),
                    transaction.source().as_str(),
                    transaction.category_id().as_string(),
                    transaction.account_id().as_string(),
                    transaction.dedup_key().as_str(),
                    now,
                    transaction.role().as_str(),
                    transaction.transfer_group_id().map(|id| id.as_string()),
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn delete(&self, id: TransactionId) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM transactions WHERE id = ?1",
                params![id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM transactions WHERE id = ?1"
            ))
            .map_err(sql_err)?;
        let mut rows = stmt
            .query_map(params![id.as_string()], |row| self.row_to_transaction(row))
            .map_err(sql_err)?;
        rows.next().transpose().map_err(sql_err)
    }

    fn delete_transfer_group(&self, group_id: TransferGroupId) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM transactions WHERE transfer_group_id = ?1",
                params![group_id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn update_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "UPDATE transactions SET category_id = ?1 WHERE id = ?2",
                params![category_id.as_string(), id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS}
                     FROM transactions WHERE date >= ?1 AND date <= ?2 ORDER BY date DESC"
            ))
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(
                params![
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string()
                ],
                |row| self.row_to_transaction(row),
            )
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(rows)
    }

    fn list_all(&self) -> Result<Vec<Transaction>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM transactions ORDER BY date DESC"
            ))
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| self.row_to_transaction(row))
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(rows)
    }

    fn list_page(&self, offset: i64, limit: i64) -> Result<Vec<Transaction>, RepositoryError> {
        // `id` breaks ties on same-day transactions — `ORDER BY date DESC`
        // alone isn't a stable order across separate LIMIT/OFFSET queries,
        // which would let a row be skipped or repeated as the caller pages
        // through.
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM transactions
                     ORDER BY date DESC, id DESC LIMIT ?1 OFFSET ?2"
            ))
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit, offset], |row| self.row_to_transaction(row))
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrat_domain::account::{Account, AccountName};
    use scrat_domain::category::{Category, CategoryName};
    use scrat_domain::ports::{AccountRepository as _, CategoryRepository as _};

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    /// Inserts an account and category directly via their real repositories
    /// so the transactions table's foreign keys are satisfied.
    fn seed_account_and_category(conn: &Connection) -> (AccountId, CategoryId) {
        let account_repo = crate::SqliteAccountRepository::new(conn, usd());
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        );
        account_repo.insert(&account).unwrap();

        let category_repo = crate::SqliteCategoryRepository::new(conn);
        let category = Category::new(
            CategoryId::new(),
            CategoryName::new("Groceries").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&category).unwrap();

        (account.id(), category.id())
    }

    #[test]
    fn persists_and_reloads_roundtrip() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let transaction = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            SourceText::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();

        repo.insert(&transaction).unwrap();
        let reloaded = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();

        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].amount().minor_units(), -1_200);
        assert_eq!(reloaded[0].source().as_str(), "Whole Foods");
    }

    #[test]
    fn list_in_range_excludes_dates_outside_range() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                Money::from_minor_units(-1_200, usd()),
                SourceText::new("In range").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                Money::from_minor_units(-500, usd()),
                SourceText::new("Out of range").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let results = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source().as_str(), "In range");
    }

    /// Regression test: `list_in_range(NaiveDate::MIN, NaiveDate::MAX)` was
    /// previously used as an "all transactions" query elsewhere in the app,
    /// but chrono formats those extreme years with a leading sign
    /// (`+262142-12-31`) that sorts before ordinary years in SQLite's TEXT
    /// comparison, so it silently matched nothing against a real database
    /// even with rows present. `list_all` must not have that problem.
    #[test]
    fn list_all_returns_every_transaction_regardless_of_date() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                Money::from_minor_units(-1_200, usd()),
                SourceText::new("Whole Foods").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2030, 6, 1).unwrap(),
                Money::from_minor_units(-500, usd()),
                SourceText::new("Far future").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let results = repo.list_all().unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_page_walks_the_whole_ledger_without_gaps_or_duplicates() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for day in 1..=25 {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
                    Money::from_minor_units(-100 * day as i64, usd()),
                    SourceText::new(&format!("Day {day}")).unwrap(),
                    category_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let mut seen = std::collections::HashSet::new();
        let mut offset = 0i64;
        loop {
            let page = repo.list_page(offset, 10).unwrap();
            if page.is_empty() {
                break;
            }
            for t in &page {
                assert!(seen.insert(t.id()), "transaction returned on two pages");
            }
            offset += page.len() as i64;
        }

        assert_eq!(seen.len(), 25);
    }

    #[test]
    fn list_page_orders_newest_first() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                Money::from_minor_units(-100, usd()),
                SourceText::new("Oldest").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
                Money::from_minor_units(-100, usd()),
                SourceText::new("Newest").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let page = repo.list_page(0, 10).unwrap();

        assert_eq!(
            page.iter().map(|t| t.source().as_str()).collect::<Vec<_>>(),
            vec!["Newest", "Oldest"]
        );
    }

    #[test]
    fn delete_removes_row() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let transaction = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            SourceText::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&transaction).unwrap();

        repo.delete(transaction.id()).unwrap();

        let results = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn update_category_changes_the_stored_row() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let transaction = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            SourceText::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&transaction).unwrap();

        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let other_category = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&other_category).unwrap();

        repo.update_category(transaction.id(), other_category.id())
            .unwrap();

        let results = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category_id(), other_category.id());
    }

    /// A stored row has to come back with the role and group it went in
    /// with — a mirrored leg that reloads as `Normal` would quietly start
    /// counting as income on the counterpart account.
    #[test]
    fn role_and_transfer_group_survive_a_roundtrip() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let group_id = TransferGroupId::new();
        let transfer = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-25_000, usd()),
            SourceText::new("Virement N26").unwrap(),
            category_id,
            account_id,
            TransactionRole::Transfer,
            Some(group_id),
        )
        .unwrap();
        repo.insert(&transfer).unwrap();

        let reloaded = repo.find_by_id(transfer.id()).unwrap().unwrap();

        assert_eq!(reloaded.role(), TransactionRole::Transfer);
        assert_eq!(reloaded.transfer_group_id(), Some(group_id));
    }

    #[test]
    fn an_ordinary_transaction_reloads_as_normal_with_no_group() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let transaction = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            SourceText::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&transaction).unwrap();

        let reloaded = repo.find_by_id(transaction.id()).unwrap().unwrap();

        assert_eq!(reloaded.role(), TransactionRole::Normal);
        assert_eq!(reloaded.transfer_group_id(), None);
    }

    #[test]
    fn find_by_id_returns_none_for_an_unknown_id() {
        let conn = test_conn();
        let repo = SqliteTransactionRepository::new(&conn, usd());

        assert!(repo.find_by_id(TransactionId::new()).unwrap().is_none());
    }

    /// Both legs go, and nothing else does.
    #[test]
    fn delete_transfer_group_removes_both_legs_and_leaves_others() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let counterpart_account_id = {
            let account_repo = crate::SqliteAccountRepository::new(&conn, usd());
            let account = Account::new(
                AccountId::new(),
                AccountName::new("Neobank").unwrap(),
                Money::zero(usd()),
            );
            account_repo.insert(&account).unwrap();
            account.id()
        };
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let group_id = TransferGroupId::new();
        let outflow = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-25_000, usd()),
            SourceText::new("Virement N26").unwrap(),
            category_id,
            account_id,
            TransactionRole::Transfer,
            Some(group_id),
        )
        .unwrap();
        let inflow = outflow
            .mirrored_onto(counterpart_account_id, group_id)
            .unwrap();
        repo.insert(&outflow).unwrap();
        repo.insert(&inflow).unwrap();
        let unrelated = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
            Money::from_minor_units(-450, usd()),
            SourceText::new("Boulangerie").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&unrelated).unwrap();

        repo.delete_transfer_group(group_id).unwrap();

        let remaining = repo.list_all().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id(), unrelated.id());
    }

    #[test]
    fn identical_account_date_amount_and_source_are_both_kept() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let make = || {
            Transaction::new(
                TransactionId::new(),
                date,
                Money::from_minor_units(-1_200, usd()),
                SourceText::new("Whole Foods").unwrap(),
                category_id,
                account_id,
            )
            .unwrap()
        };
        repo.insert(&make()).unwrap();
        repo.insert(&make()).unwrap();

        let results = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();
        assert_eq!(results.len(), 2);
    }
}
