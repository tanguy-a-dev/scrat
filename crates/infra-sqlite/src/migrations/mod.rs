use rusqlite::Connection;

/// Ordered, append-only list of migrations. Add new entries at the end;
/// never edit or remove an existing one once it has shipped.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("0001_initial.sql")),
    (2, include_str!("0002_account_opening_balance.sql")),
    (3, include_str!("0003_category_icon.sql")),
    (4, include_str!("0004_drop_transactions_dedup_unique.sql")),
    (5, include_str!("0005_transfers.sql")),
    (6, include_str!("0006_rename_source_to_description.sql")),
    (7, include_str!("0007_transaction_operation_kind.sql")),
    (8, include_str!("0008_account_opening_balance_set.sql")),
];

/// The version a freshly created database ends up at. Derived from
/// [`MIGRATIONS`] rather than written out, so adding a migration can't leave
/// a stale number behind for a test to assert against.
#[cfg(test)]
pub fn latest_version() -> i64 {
    MIGRATIONS
        .last()
        .map(|(version, _)| *version)
        .unwrap_or_default()
}

/// Applies any migration whose version isn't yet recorded in
/// `schema_migrations`, in order, each inside its own transaction. Safe to
/// call on every startup (including against an already-migrated database).
pub fn run(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    for (version, sql) in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if already_applied {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [version],
        )?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies migrations up to and including `through`, leaving the
    /// database at that version — the state a user's existing file is in
    /// before an upgrade.
    fn conn_at_version(through: i64) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )
        .unwrap();
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= through) {
            let tx = conn.transaction().unwrap();
            tx.execute_batch(sql).unwrap();
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
                [version],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        conn
    }

    fn seed_one_transaction(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO accounts (id, name, created_at, updated_at)
                 VALUES ('a', 'Checking', '2026-01-01', '2026-01-01');
             INSERT INTO categories (id, name, created_at)
                 VALUES ('c', 'Groceries', '2026-01-01');
             INSERT INTO transactions
                 (id, date, amount_minor_units, source, category_id, account_id,
                  dedup_key, created_at)
                 VALUES ('t', '2026-01-15', -1200, 'Whole Foods', 'c', 'a',
                         'key', '2026-01-15');",
        )
        .unwrap();
    }

    /// Upgrading a database that already holds a ledger must not fail and
    /// must not disturb what's in it. `role` is NOT NULL, so its default is
    /// the only thing standing between an existing row and a failed
    /// migration on the user's real file.
    #[test]
    fn migration_5_backfills_existing_transactions_as_normal() {
        let mut conn = conn_at_version(4);
        seed_one_transaction(&conn);

        run(&mut conn).unwrap();

        let (role, group_id): (String, Option<String>) = conn
            .query_row(
                "SELECT role, transfer_group_id FROM transactions WHERE id = 't'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(role, "normal");
        assert_eq!(group_id, None);
    }

    /// Note the column is read as `description`: migration 6 renamed it from
    /// `source`, which is why `seed_one_transaction` still writes the old
    /// name (that is genuinely what the column is called at version 4).
    #[test]
    fn migration_5_preserves_existing_transaction_data() {
        let mut conn = conn_at_version(4);
        seed_one_transaction(&conn);

        run(&mut conn).unwrap();

        let (amount, description): (i64, String) = conn
            .query_row(
                "SELECT amount_minor_units, description FROM transactions WHERE id = 't'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(amount, -1_200);
        assert_eq!(description, "Whole Foods");
    }

    /// The rename must carry existing values across untouched — a user's
    /// ledger is the one thing a cosmetic migration must not cost them.
    #[test]
    fn migration_6_renames_columns_without_losing_data() {
        let mut conn = conn_at_version(5);
        seed_one_transaction(&conn);

        run(&mut conn).unwrap();

        let (description, fingerprint): (String, String) = conn
            .query_row(
                "SELECT description, fingerprint FROM transactions WHERE id = 't'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(description, "Whole Foods");
        assert_eq!(fingerprint, "key");
    }

    /// The account-pattern table was renamed too; its rows must survive.
    #[test]
    fn migration_6_renames_account_pattern_table_without_losing_rows() {
        let mut conn = conn_at_version(5);
        seed_one_transaction(&conn);
        conn.execute_batch(
            "INSERT INTO account_source_patterns (id, account_id, pattern)
                 VALUES ('p', 'a', 'whole foods');",
        )
        .unwrap();

        run(&mut conn).unwrap();

        let pattern: String = conn
            .query_row(
                "SELECT pattern FROM account_description_patterns WHERE id = 'p'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pattern, "whole foods");
    }

    /// `operation_kind` is NOT NULL, so — exactly as with `role` in
    /// migration 5 — its default is the only thing between an existing
    /// ledger and a failed migration on the user's real file.
    ///
    /// Seeded at version 4 rather than 6 because that's the version
    /// `seed_one_transaction`'s column names belong to (6 renames them) —
    /// the row is just as pre-existing either way.
    #[test]
    fn migration_7_backfills_existing_transactions_as_card() {
        let mut conn = conn_at_version(4);
        seed_one_transaction(&conn);

        run(&mut conn).unwrap();

        let operation_kind: String = conn
            .query_row(
                "SELECT operation_kind FROM transactions WHERE id = 't'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(operation_kind, "card");
    }

    /// The distinction the new column exists to record. A non-zero opening
    /// balance is something the user typed; a 0 is the default they may
    /// simply never have filled in, and must not be mistaken for an answer.
    #[test]
    fn migration_8_treats_only_a_non_zero_opening_balance_as_established() {
        let mut conn = conn_at_version(7);
        conn.execute_batch(
            "INSERT INTO accounts (id, name, opening_balance_minor_units, created_at, updated_at)
                 VALUES ('anchored', 'Savings', 25000, '2026-01-01', '2026-01-01'),
                        ('unset', 'Checking', 0, '2026-01-01', '2026-01-01');",
        )
        .unwrap();

        run(&mut conn).unwrap();

        let mut stmt = conn
            .prepare("SELECT id, opening_balance_set FROM accounts ORDER BY id")
            .unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![("anchored".to_string(), 1), ("unset".to_string(), 0)]
        );
    }

    #[test]
    fn run_is_safe_to_call_again_on_an_already_migrated_database() {
        let mut conn = conn_at_version(4);
        seed_one_transaction(&conn);
        run(&mut conn).unwrap();

        run(&mut conn).unwrap();

        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(applied, MIGRATIONS.len() as i64);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
}
