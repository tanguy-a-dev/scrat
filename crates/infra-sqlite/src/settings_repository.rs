use rusqlite::{params, Connection, OptionalExtension};
use scrat_domain::ports::RepositoryError;

pub fn get_currency_code(conn: &Connection) -> Result<Option<String>, RepositoryError> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'currency_code'",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| RepositoryError(e.to_string()))
}

pub fn set_currency_code(conn: &Connection, code: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('currency_code', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![code],
    )
    .map_err(|e| RepositoryError(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    #[test]
    fn get_currency_code_returns_none_when_unset() {
        let conn = test_conn();
        assert_eq!(get_currency_code(&conn).unwrap(), None);
    }

    #[test]
    fn set_then_get_currency_code_roundtrips() {
        let conn = test_conn();
        set_currency_code(&conn, "EUR").unwrap();
        assert_eq!(get_currency_code(&conn).unwrap(), Some("EUR".to_string()));
    }

    #[test]
    fn set_currency_code_overwrites_previous_value() {
        let conn = test_conn();
        set_currency_code(&conn, "EUR").unwrap();
        set_currency_code(&conn, "GBP").unwrap();
        assert_eq!(get_currency_code(&conn).unwrap(), Some("GBP".to_string()));
    }
}
