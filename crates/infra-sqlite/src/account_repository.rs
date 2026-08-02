use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use scrat_domain::account::{Account, AccountId, AccountName, DescriptionPattern};
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{AccountRepository, RepositoryError};

pub struct SqliteAccountRepository<'a> {
    conn: &'a Connection,
    currency: Currency,
}

impl<'a> SqliteAccountRepository<'a> {
    pub fn new(conn: &'a Connection, currency: Currency) -> Self {
        Self { conn, currency }
    }

    fn load_description_patterns(
        &self,
        id: AccountId,
    ) -> Result<Vec<DescriptionPattern>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT pattern FROM account_description_patterns WHERE account_id = ?1")
            .map_err(sql_err)?;
        let patterns = stmt
            .query_map(params![id.as_string()], |row| row.get::<_, String>(0))
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        patterns
            .into_iter()
            .map(|p| DescriptionPattern::new(&p).map_err(|e| RepositoryError(e.to_string())))
            .collect()
    }

    fn row_to_account(&self, row: &rusqlite::Row) -> rusqlite::Result<(AccountId, Account)> {
        let id_str: String = row.get("id")?;
        let id = AccountId::parse(&id_str).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        let name: String = row.get("name")?;
        let opening_balance_minor_units: i64 = row.get("opening_balance_minor_units")?;
        let opening_balance_set: bool = row.get("opening_balance_set")?;

        let name = AccountName::new(&name).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        // The stored amount is meaningless until the flag says someone
        // established it — see `Account::opening_balance`.
        let opening_balance = opening_balance_set
            .then(|| Money::from_minor_units(opening_balance_minor_units, self.currency.clone()));

        Ok((
            id,
            Account::from_parts(id, name, opening_balance, Vec::new()),
        ))
    }
}

fn sql_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError(e.to_string())
}

impl<'a> AccountRepository for SqliteAccountRepository<'a> {
    fn insert(&self, account: &Account) -> Result<(), RepositoryError> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO accounts
                     (id, name, opening_balance_minor_units, opening_balance_set,
                      created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    account.id().as_string(),
                    account.name().as_str(),
                    account.opening_balance_minor_units(),
                    account.is_opening_balance_set(),
                    now,
                ],
            )
            .map_err(sql_err)?;
        self.replace_description_patterns(account)
    }

    fn update(&self, account: &Account) -> Result<(), RepositoryError> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE accounts
                    SET name = ?2, opening_balance_minor_units = ?3,
                        opening_balance_set = ?4, updated_at = ?5
                  WHERE id = ?1",
                params![
                    account.id().as_string(),
                    account.name().as_str(),
                    account.opening_balance_minor_units(),
                    account.is_opening_balance_set(),
                    now,
                ],
            )
            .map_err(sql_err)?;
        self.replace_description_patterns(account)
    }

    fn delete(&self, id: AccountId) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM accounts WHERE id = ?1",
                params![id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        let result = self
            .conn
            .query_row(
                "SELECT id, name, opening_balance_minor_units, opening_balance_set
                   FROM accounts WHERE id = ?1",
                params![id.as_string()],
                |row| self.row_to_account(row),
            )
            .optional()
            .map_err(sql_err)?;

        match result {
            Some((id, mut account)) => {
                for pattern in self.load_description_patterns(id)? {
                    account.add_description_pattern(pattern);
                }
                Ok(Some(account))
            }
            None => Ok(None),
        }
    }

    fn list_all(&self) -> Result<Vec<Account>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, opening_balance_minor_units, opening_balance_set
                   FROM accounts ORDER BY name",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| self.row_to_account(row))
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;

        rows.into_iter()
            .map(|(id, mut account)| {
                for pattern in self.load_description_patterns(id)? {
                    account.add_description_pattern(pattern);
                }
                Ok(account)
            })
            .collect()
    }

    fn transaction_count(&self, id: AccountId) -> Result<u64, RepositoryError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE account_id = ?1",
                params![id.as_string()],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(sql_err)
    }

    fn sum_transactions_minor_units(&self, id: AccountId) -> Result<i64, RepositoryError> {
        self.conn
            .query_row(
                "SELECT COALESCE(SUM(amount_minor_units), 0) FROM transactions WHERE account_id = ?1",
                params![id.as_string()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_err)
    }
}

impl<'a> SqliteAccountRepository<'a> {
    fn replace_description_patterns(&self, account: &Account) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM account_description_patterns WHERE account_id = ?1",
                params![account.id().as_string()],
            )
            .map_err(sql_err)?;
        for pattern in account.description_patterns() {
            self.conn
                .execute(
                    "INSERT INTO account_description_patterns (id, account_id, pattern) VALUES (?1, ?2, ?3)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        account.id().as_string(),
                        pattern.as_str(),
                    ],
                )
                .map_err(sql_err)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        // Leak the tempdir so the file survives for the lifetime of the test
        // (its own process-exit cleanup is fine for a test binary).
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    #[test]
    fn persists_and_reloads_roundtrip() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::from_minor_units(12_345, usd()),
        );
        account.add_description_pattern(DescriptionPattern::new("acme corp").unwrap());

        repo.insert(&account).unwrap();
        let reloaded = repo.find_by_id(account.id()).unwrap().unwrap();

        assert_eq!(reloaded.name().as_str(), "Checking");
        assert_eq!(reloaded.opening_balance_minor_units(), 12_345);
        assert!(reloaded.is_opening_balance_set());
        assert_eq!(reloaded.description_patterns().len(), 1);
        assert_eq!(reloaded.description_patterns()[0].as_str(), "acme corp");
    }

    /// The flag has to survive the round trip on its own, because the amount
    /// column can't carry it: an unestablished anchor and one deliberately
    /// set to zero both store 0.
    #[test]
    fn an_unestablished_starting_point_survives_a_roundtrip_distinct_from_zero() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let unanchored = Account::without_opening_balance(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
        );
        let mut anchored_at_zero = Account::new(
            AccountId::new(),
            AccountName::new("Savings").unwrap(),
            Money::zero(usd()),
        );

        repo.insert(&unanchored).unwrap();
        repo.insert(&anchored_at_zero).unwrap();

        let reloaded = repo.find_by_id(unanchored.id()).unwrap().unwrap();
        assert!(!reloaded.is_opening_balance_set());
        assert_eq!(reloaded.opening_balance(), None);
        assert!(repo
            .find_by_id(anchored_at_zero.id())
            .unwrap()
            .unwrap()
            .is_opening_balance_set());

        // And establishing it later has to stick through `update`, not just
        // `insert` — that's the path the UI actually takes.
        anchored_at_zero.set_opening_balance(Money::from_minor_units(500, usd()));
        let mut now_anchored = unanchored.clone();
        now_anchored.set_opening_balance(Money::zero(usd()));
        repo.update(&now_anchored).unwrap();

        let reloaded = repo.find_by_id(unanchored.id()).unwrap().unwrap();
        assert!(reloaded.is_opening_balance_set());
        assert_eq!(reloaded.opening_balance_minor_units(), 0);
    }

    #[test]
    fn update_replaces_description_patterns() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        );
        account.add_description_pattern(DescriptionPattern::new("old pattern").unwrap());
        repo.insert(&account).unwrap();

        account.remove_description_pattern(&DescriptionPattern::new("old pattern").unwrap());
        account.add_description_pattern(DescriptionPattern::new("new pattern").unwrap());
        repo.update(&account).unwrap();

        let reloaded = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(reloaded.description_patterns().len(), 1);
        assert_eq!(reloaded.description_patterns()[0].as_str(), "new pattern");
    }

    #[test]
    fn delete_removes_row() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        );
        repo.insert(&account).unwrap();

        repo.delete(account.id()).unwrap();

        assert!(repo.find_by_id(account.id()).unwrap().is_none());
    }

    #[test]
    fn sum_transactions_returns_zero_when_none() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        );
        repo.insert(&account).unwrap();

        assert_eq!(repo.sum_transactions_minor_units(account.id()).unwrap(), 0);
        assert_eq!(repo.transaction_count(account.id()).unwrap(), 0);
    }

    #[test]
    fn transaction_count_and_sum_reflect_inserted_ledger_rows() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        );
        repo.insert(&account).unwrap();

        // Insert raw category + transaction rows directly — Category/Transaction
        // repositories don't exist yet (M3/M4), this only exercises the SQL.
        conn.execute(
            "INSERT INTO categories (id, name, parent_id, created_at) VALUES ('cat-1', 'Groceries', NULL, datetime('now'))",
            [],
        )
        .unwrap();
        for (i, amount) in [(-2_000_i64), (-500)].into_iter().enumerate() {
            conn.execute(
                "INSERT INTO transactions (id, date, amount_minor_units, description, category_id, account_id, fingerprint, created_at)
                 VALUES (?1, '2026-01-01', ?2, 'Store', 'cat-1', ?3, ?4, datetime('now'))",
                params![format!("tx-{i}"), amount, account.id().as_string(), format!("fp-{i}")],
            )
            .unwrap();
        }

        assert_eq!(repo.transaction_count(account.id()).unwrap(), 2);
        assert_eq!(
            repo.sum_transactions_minor_units(account.id()).unwrap(),
            -2_500
        );
    }

    #[test]
    fn list_all_orders_by_name() {
        let conn = test_conn();
        let repo = SqliteAccountRepository::new(&conn, usd());
        repo.insert(&Account::new(
            AccountId::new(),
            AccountName::new("Savings").unwrap(),
            Money::zero(usd()),
        ))
        .unwrap();
        repo.insert(&Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(usd()),
        ))
        .unwrap();

        let all = repo.list_all().unwrap();

        assert_eq!(
            all.iter().map(|a| a.name().as_str()).collect::<Vec<_>>(),
            vec!["Checking", "Savings"]
        );
    }
}
