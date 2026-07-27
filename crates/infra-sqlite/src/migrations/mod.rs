use rusqlite::Connection;

/// Ordered, append-only list of migrations. Add new entries at the end;
/// never edit or remove an existing one once it has shipped.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("0001_initial.sql")),
    (2, include_str!("0002_account_opening_balance.sql")),
];

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
