use rusqlite::{Connection, OptionalExtension, params};
use scrat_domain::ports::RepositoryError;

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, RepositoryError> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| RepositoryError(e.to_string()))
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| RepositoryError(e.to_string()))?;
    Ok(())
}

pub fn get_currency_code(conn: &Connection) -> Result<Option<String>, RepositoryError> {
    get_setting(conn, "currency_code")
}

pub fn set_currency_code(conn: &Connection, code: &str) -> Result<(), RepositoryError> {
    set_setting(conn, "currency_code", code)
}

pub fn get_default_account_id(conn: &Connection) -> Result<Option<String>, RepositoryError> {
    get_setting(conn, "default_account_id")
}

pub fn set_default_account_id(conn: &Connection, id: &str) -> Result<(), RepositoryError> {
    set_setting(conn, "default_account_id", id)
}

/// Stored as the decimal string form of the minutes value (`"0"` for
/// "never"). `None` means nobody has changed it from the app default yet.
pub fn get_auto_lock_minutes(conn: &Connection) -> Result<Option<String>, RepositoryError> {
    get_setting(conn, "auto_lock_minutes")
}

pub fn set_auto_lock_minutes(conn: &Connection, minutes: &str) -> Result<(), RepositoryError> {
    set_setting(conn, "auto_lock_minutes", minutes)
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

    #[test]
    fn get_default_account_id_returns_none_when_unset() {
        let conn = test_conn();
        assert_eq!(get_default_account_id(&conn).unwrap(), None);
    }

    #[test]
    fn set_then_get_default_account_id_roundtrips() {
        let conn = test_conn();
        set_default_account_id(&conn, "acc-1").unwrap();
        assert_eq!(
            get_default_account_id(&conn).unwrap(),
            Some("acc-1".to_string())
        );
    }

    #[test]
    fn set_default_account_id_overwrites_previous_value() {
        let conn = test_conn();
        set_default_account_id(&conn, "acc-1").unwrap();
        set_default_account_id(&conn, "acc-2").unwrap();
        assert_eq!(
            get_default_account_id(&conn).unwrap(),
            Some("acc-2".to_string())
        );
    }

    #[test]
    fn get_auto_lock_minutes_returns_none_when_unset() {
        let conn = test_conn();
        assert_eq!(get_auto_lock_minutes(&conn).unwrap(), None);
    }

    #[test]
    fn set_then_get_auto_lock_minutes_roundtrips() {
        let conn = test_conn();
        set_auto_lock_minutes(&conn, "60").unwrap();
        assert_eq!(
            get_auto_lock_minutes(&conn).unwrap(),
            Some("60".to_string())
        );
    }

    #[test]
    fn set_auto_lock_minutes_overwrites_previous_value() {
        let conn = test_conn();
        set_auto_lock_minutes(&conn, "10").unwrap();
        set_auto_lock_minutes(&conn, "0").unwrap();
        assert_eq!(get_auto_lock_minutes(&conn).unwrap(), Some("0".to_string()));
    }
}
