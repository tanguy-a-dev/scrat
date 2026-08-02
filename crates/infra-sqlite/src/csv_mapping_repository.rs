//! Storage for CSV column mappings the user has corrected and committed.
//!
//! Free functions rather than a repository struct implementing a port, for
//! the same reason `settings_repository` is: there is no aggregate here.
//! Which column of a bank export holds the amount is an adapter concern with
//! no domain invariants attached — modelling it in `domain` would put CSV
//! files into a layer that is supposed to know nothing about them. The
//! composition root (`src-tauri`) calls these directly, exactly as it already
//! does for settings and default-id lookups.
//!
//! The mapping is stored as an opaque string. This module deliberately does
//! not know its structure; `src-tauri` owns the DTO and its serialization.

use rusqlite::{params, Connection, OptionalExtension};
use scrat_domain::ports::RepositoryError;

/// The mapping last committed for this file layout, if any. See
/// `file_signature` in `scrat-infra-csv` for what a signature identifies.
pub fn get_csv_mapping(
    conn: &Connection,
    signature: &str,
) -> Result<Option<String>, RepositoryError> {
    conn.query_row(
        "SELECT mapping_json FROM csv_import_mappings WHERE signature = ?1",
        params![signature],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| RepositoryError(e.to_string()))
}

/// Records the mapping an import was actually committed with, replacing any
/// previous one for the same layout — which is what lets a remembered
/// mapping that turned out to be wrong heal itself as soon as the user
/// corrects it and imports again.
pub fn save_csv_mapping(
    conn: &Connection,
    signature: &str,
    mapping_json: &str,
) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO csv_import_mappings (signature, mapping_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(signature) DO UPDATE SET
             mapping_json = excluded.mapping_json,
             updated_at = excluded.updated_at",
        params![signature, mapping_json, chrono::Utc::now().to_rfc3339()],
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
    fn get_returns_none_for_an_unseen_layout() {
        let conn = test_conn();
        assert_eq!(get_csv_mapping(&conn, "h1:date\u{1f}amount").unwrap(), None);
    }

    #[test]
    fn save_then_get_roundtrips() {
        let conn = test_conn();
        save_csv_mapping(&conn, "h1:date\u{1f}amount", r#"{"date_column":0}"#).unwrap();
        assert_eq!(
            get_csv_mapping(&conn, "h1:date\u{1f}amount").unwrap(),
            Some(r#"{"date_column":0}"#.to_string())
        );
    }

    /// The self-healing property: committing a corrected mapping must
    /// replace the remembered one, not accumulate a second row that the
    /// primary key would reject anyway.
    #[test]
    fn saving_again_replaces_the_remembered_mapping() {
        let conn = test_conn();
        save_csv_mapping(&conn, "sig", r#"{"amount":1}"#).unwrap();
        save_csv_mapping(&conn, "sig", r#"{"amount":8}"#).unwrap();

        assert_eq!(
            get_csv_mapping(&conn, "sig").unwrap(),
            Some(r#"{"amount":8}"#.to_string())
        );
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM csv_import_mappings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn mappings_for_different_layouts_are_kept_apart() {
        let conn = test_conn();
        save_csv_mapping(&conn, "h1:caisse", r#"{"a":1}"#).unwrap();
        save_csv_mapping(&conn, "s1:;:dnt", r#"{"a":2}"#).unwrap();

        assert_eq!(
            get_csv_mapping(&conn, "h1:caisse").unwrap(),
            Some(r#"{"a":1}"#.to_string())
        );
        assert_eq!(
            get_csv_mapping(&conn, "s1:;:dnt").unwrap(),
            Some(r#"{"a":2}"#.to_string())
        );
    }
}
