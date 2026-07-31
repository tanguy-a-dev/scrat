use rusqlite::{params, Connection};
use scrat_domain::account::{AccountId, SourcePattern};
use scrat_domain::ports::{RepositoryError, TransferRuleRepository};
use scrat_domain::transfer_rule::{TransferRule, TransferRuleId};

pub struct SqliteTransferRuleRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteTransferRuleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

fn invalid_column<E: std::fmt::Display>(e: E) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
}

fn sql_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError(e.to_string())
}

fn row_to_rule(row: &rusqlite::Row) -> rusqlite::Result<TransferRule> {
    let id: String = row.get("id")?;
    let id = TransferRuleId::parse(&id).map_err(invalid_column)?;
    let pattern: String = row.get("pattern")?;
    let pattern = SourcePattern::new(&pattern).map_err(invalid_column)?;
    let counterpart_account_id: String = row.get("counterpart_account_id")?;
    let counterpart_account_id =
        AccountId::parse(&counterpart_account_id).map_err(invalid_column)?;
    Ok(TransferRule::new(id, pattern, counterpart_account_id))
}

impl<'a> TransferRuleRepository for SqliteTransferRuleRepository<'a> {
    fn insert(&self, rule: &TransferRule) -> Result<(), RepositoryError> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO transfer_rules (id, pattern, counterpart_account_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    rule.id().as_string(),
                    rule.pattern().as_str(),
                    rule.counterpart_account_id().as_string(),
                    now,
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn delete(&self, id: TransferRuleId) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM transfer_rules WHERE id = ?1",
                params![id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn list_all(&self) -> Result<Vec<TransferRule>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, pattern, counterpart_account_id
                 FROM transfer_rules ORDER BY pattern",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], row_to_rule)
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
    use scrat_domain::money::{Currency, Money};
    use scrat_domain::ports::AccountRepository as _;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn seed_account(conn: &Connection, name: &str) -> AccountId {
        let repo = crate::SqliteAccountRepository::new(conn, eur());
        let account = Account::new(
            AccountId::new(),
            AccountName::new(name).unwrap(),
            Money::zero(eur()),
        );
        repo.insert(&account).unwrap();
        account.id()
    }

    #[test]
    fn persists_and_reloads_roundtrip() {
        let conn = test_conn();
        let account_id = seed_account(&conn, "N26");
        let repo = SqliteTransferRuleRepository::new(&conn);
        let rule = TransferRule::new(
            TransferRuleId::new(),
            SourcePattern::new("N26").unwrap(),
            account_id,
        );

        repo.insert(&rule).unwrap();
        let reloaded = repo.list_all().unwrap();

        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].id(), rule.id());
        assert_eq!(reloaded[0].pattern().as_str(), "n26");
        assert_eq!(reloaded[0].counterpart_account_id(), account_id);
    }

    #[test]
    fn delete_removes_the_row() {
        let conn = test_conn();
        let account_id = seed_account(&conn, "N26");
        let repo = SqliteTransferRuleRepository::new(&conn);
        let rule = TransferRule::new(
            TransferRuleId::new(),
            SourcePattern::new("n26").unwrap(),
            account_id,
        );
        repo.insert(&rule).unwrap();

        repo.delete(rule.id()).unwrap();

        assert!(repo.list_all().unwrap().is_empty());
    }

    /// The application layer checks for a duplicate pattern and returns a
    /// readable error, but the constraint is enforced in the schema too —
    /// two rules sending the same source text to different accounts would
    /// make import order decide where the money went.
    #[test]
    fn the_same_pattern_cannot_be_claimed_twice() {
        let conn = test_conn();
        let first_account = seed_account(&conn, "N26");
        let second_account = seed_account(&conn, "Revolut");
        let repo = SqliteTransferRuleRepository::new(&conn);
        repo.insert(&TransferRule::new(
            TransferRuleId::new(),
            SourcePattern::new("n26").unwrap(),
            first_account,
        ))
        .unwrap();

        let result = repo.insert(&TransferRule::new(
            TransferRuleId::new(),
            SourcePattern::new("n26").unwrap(),
            second_account,
        ));

        assert!(result.is_err());
    }
}
