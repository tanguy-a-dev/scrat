use chrono::NaiveDate;
use rusqlite::{params, Connection};
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{InsertOutcome, RepositoryError, TransactionRepository};
use scrat_domain::transaction::{SourceText, Transaction, TransactionId};

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

        let amount = Money::from_minor_units(amount_minor_units, self.currency.clone());
        Transaction::new(id, date, amount, source, category_id, account_id).map_err(invalid_column)
    }
}

fn invalid_column<E: std::fmt::Display>(e: E) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
}

fn sql_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError(e.to_string())
}

const INSERT_SQL: &str = "INSERT INTO transactions
    (id, date, amount_minor_units, source, category_id, account_id, dedup_key, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

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
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn insert_or_skip(&self, transaction: &Transaction) -> Result<InsertOutcome, RepositoryError> {
        let date = transaction.date().format("%Y-%m-%d").to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let rows_changed = self
            .conn
            .execute(
                &format!("{INSERT_SQL} ON CONFLICT(dedup_key) DO NOTHING"),
                params![
                    transaction.id().as_string(),
                    date,
                    transaction.amount().minor_units(),
                    transaction.source().as_str(),
                    transaction.category_id().as_string(),
                    transaction.account_id().as_string(),
                    transaction.dedup_key().as_str(),
                    now,
                ],
            )
            .map_err(sql_err)?;
        Ok(if rows_changed == 0 {
            InsertOutcome::DuplicateSkipped
        } else {
            InsertOutcome::Inserted
        })
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

    fn delete_in_range(&self, start: NaiveDate, end: NaiveDate) -> Result<u64, RepositoryError> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM transactions WHERE date >= ?1 AND date <= ?2",
                params![
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string()
                ],
            )
            .map_err(sql_err)?;
        Ok(deleted as u64)
    }

    fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, date, amount_minor_units, source, category_id, account_id
                 FROM transactions WHERE date >= ?1 AND date <= ?2 ORDER BY date DESC",
            )
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
    fn delete_in_range_removes_only_matching_rows_and_returns_count() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let make = |date: NaiveDate, source: &str| {
            Transaction::new(
                TransactionId::new(),
                date,
                Money::from_minor_units(-500, usd()),
                SourceText::new(source).unwrap(),
                category_id,
                account_id,
            )
            .unwrap()
        };
        repo.insert(&make(NaiveDate::from_ymd_opt(2023, 4, 4).unwrap(), "In range 1"))
            .unwrap();
        repo.insert(&make(NaiveDate::from_ymd_opt(2023, 4, 5).unwrap(), "In range 2"))
            .unwrap();
        repo.insert(&make(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), "Out of range"))
            .unwrap();

        let deleted = repo
            .delete_in_range(
                NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            )
            .unwrap();

        assert_eq!(deleted, 2);
        let remaining = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2001, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2100, 1, 1).unwrap(),
            )
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].source().as_str(), "Out of range");
    }

    #[test]
    fn duplicate_dedup_key_is_rejected_by_unique_constraint() {
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

        let result = repo.insert(&make());

        assert!(result.is_err());
    }

    #[test]
    fn insert_or_skip_reports_duplicate_instead_of_erroring() {
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

        let first = repo.insert_or_skip(&make()).unwrap();
        let second = repo.insert_or_skip(&make()).unwrap();

        assert_eq!(first, InsertOutcome::Inserted);
        assert_eq!(second, InsertOutcome::DuplicateSkipped);
        let results = repo
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();
        assert_eq!(results.len(), 1);
    }
}
