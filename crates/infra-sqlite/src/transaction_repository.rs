use chrono::NaiveDate;
use rusqlite::{params, params_from_iter, Connection};
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{RepositoryError, TransactionRepository};
use scrat_domain::transaction::{
    Description, Transaction, TransactionId, TransactionRole, TransferGroupId,
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
        let description: String = row.get("description")?;
        let description = Description::new(&description).map_err(invalid_column)?;
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
            description,
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
    (id, date, amount_minor_units, description, category_id, account_id, fingerprint, created_at,
     role, transfer_group_id)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Every column `row_to_transaction` reads, so the three read paths can't
/// drift apart and silently drop a role or a transfer group.
const SELECT_COLUMNS: &str = "id, date, amount_minor_units, description, category_id, account_id,
     role, transfer_group_id";

/// Keeps each bulk `IN (...)` clause well under SQLite's variable-count
/// limit (historically 999, `SQLITE_MAX_VARIABLE_NUMBER`), regardless of how
/// many ids a caller passes in one call.
const ID_CHUNK_SIZE: usize = 500;

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
                    transaction.description().as_str(),
                    transaction.category_id().as_string(),
                    transaction.account_id().as_string(),
                    transaction.fingerprint().as_str(),
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

    fn delete_many(&self, ids: &[TransactionId]) -> Result<(), RepositoryError> {
        if ids.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
        for chunk in ids.chunks(ID_CHUNK_SIZE) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM transactions WHERE id IN ({placeholders})");
            let id_strings: Vec<String> = chunk.iter().map(|id| id.as_string()).collect();
            tx.execute(&sql, params_from_iter(id_strings.iter()))
                .map_err(sql_err)?;
        }
        tx.commit().map_err(sql_err)?;
        Ok(())
    }

    fn update_category_many(
        &self,
        ids: &[TransactionId],
        category_id: CategoryId,
    ) -> Result<(), RepositoryError> {
        if ids.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
        for chunk in ids.chunks(ID_CHUNK_SIZE) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql =
                format!("UPDATE transactions SET category_id = ? WHERE id IN ({placeholders})");
            let mut chunk_params: Vec<String> = Vec::with_capacity(chunk.len() + 1);
            chunk_params.push(category_id.as_string());
            chunk_params.extend(chunk.iter().map(|id| id.as_string()));
            tx.execute(&sql, params_from_iter(chunk_params.iter()))
                .map_err(sql_err)?;
        }
        tx.commit().map_err(sql_err)?;
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

    fn list_page(
        &self,
        offset: i64,
        limit: i64,
        category_id: Option<CategoryId>,
        description_contains: Option<&str>,
        is_income: Option<bool>,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        // `id` breaks ties on same-day transactions — `ORDER BY date DESC`
        // alone isn't a stable order across separate LIMIT/OFFSET queries,
        // which would let a row be skipped or repeated as the caller pages
        // through.
        //
        // The three filters use the same `?N IS NULL OR …` shape as
        // `count_in_range`, so a page and the header count it sits under are
        // answering the identical question.
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM transactions
                     WHERE (?3 IS NULL OR category_id = ?3)
                       AND (?4 IS NULL OR LOWER(description) LIKE '%' || LOWER(?4) || '%')
                       AND (?5 IS NULL OR (?5 = 1 AND amount_minor_units > 0)
                                        OR (?5 = 0 AND amount_minor_units < 0))
                     ORDER BY date DESC, id DESC LIMIT ?1 OFFSET ?2"
            ))
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(
                params![
                    limit,
                    offset,
                    category_id.map(|id| id.as_string()),
                    description_contains,
                    is_income,
                ],
                |row| self.row_to_transaction(row),
            )
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(rows)
    }

    fn count_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        category_id: Option<CategoryId>,
        description_contains: Option<&str>,
        is_income: Option<bool>,
    ) -> Result<i64, RepositoryError> {
        let count = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM transactions
                     WHERE date >= ?1 AND date <= ?2
                       AND (?3 IS NULL OR category_id = ?3)
                       AND (?4 IS NULL OR LOWER(description) LIKE '%' || LOWER(?4) || '%')
                       AND (?5 IS NULL OR (?5 = 1 AND amount_minor_units > 0)
                                        OR (?5 = 0 AND amount_minor_units < 0))",
                params![
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                    category_id.map(|id| id.as_string()),
                    description_contains,
                    is_income,
                ],
                |row| row.get(0),
            )
            .map_err(sql_err)?;
        Ok(count)
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
            Description::new("Whole Foods").unwrap(),
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
        assert_eq!(reloaded[0].description().as_str(), "Whole Foods");
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
                Description::new("In range").unwrap(),
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
                Description::new("Out of range").unwrap(),
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
        assert_eq!(results[0].description().as_str(), "In range");
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
                Description::new("Whole Foods").unwrap(),
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
                Description::new("Far future").unwrap(),
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
                    Description::new(&format!("Day {day}")).unwrap(),
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
            let page = repo.list_page(offset, 10, None, None, None).unwrap();
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
                Description::new("Oldest").unwrap(),
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
                Description::new("Newest").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let page = repo.list_page(0, 10, None, None, None).unwrap();

        assert_eq!(
            page.iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Newest", "Oldest"]
        );
    }

    /// Regression test for the transactions view's category filter. The
    /// filter used to be applied in the frontend, over only the pages that
    /// had been fetched so far — so picking a category whose transactions
    /// all sit deeper in the ledger (here: every income row is older than a
    /// full page of expenses) showed an empty list until the user had
    /// scrolled the whole ledger in. Pushed down to the query, the first
    /// page of a filtered walk holds the matches themselves, wherever in
    /// the ledger they live.
    #[test]
    fn list_page_filtered_by_category_reaches_matches_beyond_the_first_page() {
        let conn = test_conn();
        let (account_id, groceries_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let salary = Category::new(
            CategoryId::new(),
            CategoryName::new("Salary").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&salary).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());

        // 25 recent expenses — more than two 10-row pages of them.
        for day in 1..=25 {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 2, day).unwrap(),
                    Money::from_minor_units(-100 * day as i64, usd()),
                    Description::new(&format!("Supermarket {day}")).unwrap(),
                    groceries_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }
        // …and 3 older income rows, which a newest-first unfiltered walk
        // would only reach on its third page.
        for month in 1..=3 {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2025, month, 28).unwrap(),
                    Money::from_minor_units(250_000, usd()),
                    Description::new(&format!("Employer {month}")).unwrap(),
                    salary.id(),
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let first_page = repo
            .list_page(0, 10, Some(salary.id()), None, None)
            .unwrap();

        assert_eq!(first_page.len(), 3);
        assert!(first_page.iter().all(|t| t.category_id() == salary.id()));
        assert_eq!(
            first_page
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Employer 3", "Employer 2", "Employer 1"],
            "a filtered page is still newest-first"
        );
    }

    /// The filtered walk has to page like the unfiltered one does — the
    /// offset counts matching rows, so no match is skipped or repeated even
    /// when non-matching rows are interleaved between them.
    #[test]
    fn list_page_paginates_within_the_filtered_set() {
        let conn = test_conn();
        let (account_id, groceries_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let salary = Category::new(
            CategoryId::new(),
            CategoryName::new("Salary").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&salary).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());

        // Alternating categories, so every filtered page straddles rows it
        // must skip over.
        for day in 1..=24 {
            let income = day % 2 == 0;
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
                    Money::from_minor_units(if income { 1_000 } else { -1_000 }, usd()),
                    Description::new(&format!("Row {day}")).unwrap(),
                    if income { salary.id() } else { groceries_id },
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let mut seen = std::collections::HashSet::new();
        let mut offset = 0i64;
        loop {
            let page = repo
                .list_page(offset, 5, Some(salary.id()), None, None)
                .unwrap();
            if page.is_empty() {
                break;
            }
            for t in &page {
                assert_eq!(t.category_id(), salary.id());
                assert!(seen.insert(t.id()), "transaction returned on two pages");
            }
            offset += page.len() as i64;
        }

        assert_eq!(seen.len(), 12, "every income row, exactly once");
    }

    #[test]
    fn list_page_filters_by_description_case_insensitively() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for description in ["WHOLE FOODS MARKET", "Electric Co", "whole foods #22"] {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                    Money::from_minor_units(-100, usd()),
                    Description::new(description).unwrap(),
                    category_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let page = repo
            .list_page(0, 10, None, Some("Whole Foods"), None)
            .unwrap();

        assert_eq!(page.len(), 2);
        assert!(page.iter().all(|t| t
            .description()
            .as_str()
            .to_lowercase()
            .contains("whole foods")));
    }

    /// Both filters at once narrow further, rather than one quietly
    /// replacing the other.
    #[test]
    fn list_page_applies_category_and_description_filters_together() {
        let conn = test_conn();
        let (account_id, groceries_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let salary = Category::new(
            CategoryId::new(),
            CategoryName::new("Salary").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&salary).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for (description, category_id) in [
            ("Employer payroll", salary.id()),
            ("Employer canteen", groceries_id),
            ("Side gig payroll", salary.id()),
        ] {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                    Money::from_minor_units(1_000, usd()),
                    Description::new(description).unwrap(),
                    category_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let page = repo
            .list_page(0, 10, Some(salary.id()), Some("employer"), None)
            .unwrap();

        assert_eq!(
            page.iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Employer payroll"]
        );
    }

    /// A filtered page and the header count above it must be answering the
    /// same question — the view shows one on top of the other, and a
    /// mismatch reads as missing transactions.
    #[test]
    fn list_page_and_count_in_range_agree_on_the_same_filters() {
        let conn = test_conn();
        let (account_id, groceries_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let salary = Category::new(
            CategoryId::new(),
            CategoryName::new("Salary").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&salary).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for day in 1..=9 {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
                    Money::from_minor_units(1_000, usd()),
                    Description::new(&format!("Row {day}")).unwrap(),
                    if day % 3 == 0 {
                        salary.id()
                    } else {
                        groceries_id
                    },
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let page = repo
            .list_page(0, 100, Some(salary.id()), None, None)
            .unwrap();
        let count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                Some(salary.id()),
                None,
                None,
            )
            .unwrap();

        assert_eq!(page.len() as i64, count);
    }

    #[test]
    fn count_in_range_respects_the_date_bounds() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                Money::from_minor_units(-100, usd()),
                Description::new("In range").unwrap(),
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
                Money::from_minor_units(-100, usd()),
                Description::new("Out of range").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                None,
                None,
                None,
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn count_in_range_filters_by_category() {
        let conn = test_conn();
        let (account_id, groceries_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let rent =
            Category::new(CategoryId::new(), CategoryName::new("Rent").unwrap(), None).unwrap();
        category_repo.insert(&rent).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                Money::from_minor_units(-100, usd()),
                Description::new("Supermarket").unwrap(),
                groceries_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                Money::from_minor_units(-90_000, usd()),
                Description::new("Landlord").unwrap(),
                rent.id(),
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                Some(groceries_id),
                None,
                None,
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn count_in_range_filters_by_description_case_insensitively() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                Money::from_minor_units(-100, usd()),
                Description::new("WHOLE FOODS MARKET").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();
        repo.insert(
            &Transaction::new(
                TransactionId::new(),
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                Money::from_minor_units(-100, usd()),
                Description::new("Electric Co").unwrap(),
                category_id,
                account_id,
            )
            .unwrap(),
        )
        .unwrap();

        let count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                None,
                Some("whole foods"),
                None,
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    /// Expenses and Income page independently through the ledger with their
    /// own filters — `is_income` is what lets a caller ask for just one
    /// sign's rows instead of a mixed-sign batch it would then have to split
    /// (and lose pagination correctness doing so).
    #[test]
    fn list_page_filters_by_sign() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for (day, description, amount) in [
            (13, "Paycheck", 2_000),
            (14, "Groceries", -500),
            (15, "Rent", -900),
        ] {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
                    Money::from_minor_units(amount, usd()),
                    Description::new(description).unwrap(),
                    category_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let income = repo.list_page(0, 10, None, None, Some(true)).unwrap();
        let expenses = repo.list_page(0, 10, None, None, Some(false)).unwrap();

        assert_eq!(
            income
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Paycheck"]
        );
        assert_eq!(
            expenses
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Rent", "Groceries"]
        );
    }

    #[test]
    fn count_in_range_filters_by_sign() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        for (description, amount) in [("Paycheck", 2_000), ("Groceries", -500), ("Rent", -900)] {
            repo.insert(
                &Transaction::new(
                    TransactionId::new(),
                    NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                    Money::from_minor_units(amount, usd()),
                    Description::new(description).unwrap(),
                    category_id,
                    account_id,
                )
                .unwrap(),
            )
            .unwrap();
        }

        let income_count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                None,
                None,
                Some(true),
            )
            .unwrap();
        let expense_count = repo
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                None,
                None,
                Some(false),
            )
            .unwrap();

        assert_eq!(income_count, 1);
        assert_eq!(expense_count, 2);
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
            Description::new("Whole Foods").unwrap(),
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
            Description::new("Whole Foods").unwrap(),
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
            Description::new("Virement N26").unwrap(),
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
            Description::new("Whole Foods").unwrap(),
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
            Description::new("Virement N26").unwrap(),
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
            Description::new("Boulangerie").unwrap(),
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
    fn delete_many_is_a_no_op_on_an_empty_list() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let kept = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            Description::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&kept).unwrap();

        repo.delete_many(&[]).unwrap();

        assert_eq!(repo.list_all().unwrap().len(), 1);
    }

    #[test]
    fn delete_many_ignores_ids_that_do_not_exist() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let kept = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            Description::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        let removed = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
            Money::from_minor_units(-450, usd()),
            Description::new("Boulangerie").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&kept).unwrap();
        repo.insert(&removed).unwrap();

        repo.delete_many(&[removed.id(), TransactionId::new()])
            .unwrap();

        let remaining = repo.list_all().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id(), kept.id());
    }

    /// Regression test for SQLite's variable-count limit on an `IN (...)`
    /// clause (historically 999, `SQLITE_MAX_VARIABLE_NUMBER`) — a page of
    /// the ledger loaded in the "All Time" view can comfortably exceed the
    /// single-chunk size, so `delete_many` must split into multiple
    /// statements and still delete every row.
    #[test]
    fn delete_many_handles_more_ids_than_one_chunk() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let mut ids = Vec::new();
        for i in 0..1_200 {
            let transaction = Transaction::new(
                TransactionId::new(),
                date,
                Money::from_minor_units(-(i + 1), usd()),
                Description::new("Bulk row").unwrap(),
                category_id,
                account_id,
            )
            .unwrap();
            repo.insert(&transaction).unwrap();
            ids.push(transaction.id());
        }

        repo.delete_many(&ids).unwrap();

        assert!(repo.list_all().unwrap().is_empty());
    }

    #[test]
    fn update_category_many_updates_only_the_listed_rows() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let category_repo = crate::SqliteCategoryRepository::new(&conn);
        let dining = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        category_repo.insert(&dining).unwrap();
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let a = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            Money::from_minor_units(-1_200, usd()),
            Description::new("Whole Foods").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        let b = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
            Money::from_minor_units(-450, usd()),
            Description::new("Boulangerie").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        let untouched = Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 17).unwrap(),
            Money::from_minor_units(-300, usd()),
            Description::new("Cafe").unwrap(),
            category_id,
            account_id,
        )
        .unwrap();
        repo.insert(&a).unwrap();
        repo.insert(&b).unwrap();
        repo.insert(&untouched).unwrap();

        repo.update_category_many(&[a.id(), b.id()], dining.id())
            .unwrap();

        let all = repo.list_all().unwrap();
        assert_eq!(
            all.iter().find(|t| t.id() == a.id()).unwrap().category_id(),
            dining.id()
        );
        assert_eq!(
            all.iter().find(|t| t.id() == b.id()).unwrap().category_id(),
            dining.id()
        );
        assert_eq!(
            all.iter()
                .find(|t| t.id() == untouched.id())
                .unwrap()
                .category_id(),
            category_id
        );
    }

    #[test]
    fn identical_account_date_amount_and_description_are_both_kept() {
        let conn = test_conn();
        let (account_id, category_id) = seed_account_and_category(&conn);
        let repo = SqliteTransactionRepository::new(&conn, usd());
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let make = || {
            Transaction::new(
                TransactionId::new(),
                date,
                Money::from_minor_units(-1_200, usd()),
                Description::new("Whole Foods").unwrap(),
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
