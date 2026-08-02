//! Optional unlock-via-1Password integration.
//!
//! Scrat still never writes the passphrase anywhere. What is stored on disk
//! here is a *secret reference* (`op://vault/item/field`) — a pointer, not a
//! secret. The passphrase itself is fetched from the user's own 1Password
//! vault at unlock time, lives in memory for the length of one command, and
//! is handed straight to SQLCipher.
//!
//! Deliberately: the fetched passphrase is never returned to the frontend.
//! `unlock_with_1password` does the read *and* the unlock in one Rust-side
//! step, so the secret never crosses the IPC boundary into JS.
//!
//! This is a composition-root concern, not a domain one — the domain has no
//! notion of a passphrase at all (see `db.rs`, which likewise talks to
//! `infra-sqlite` directly).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::db::{db_path, describe, DbState};

const CONFIG_FILE: &str = "onepassword.json";

#[derive(Serialize, Deserialize, Default)]
struct OnePasswordConfig {
    /// An `op://vault/item/field` reference. Names where the passphrase
    /// lives; is not itself sensitive.
    secret_reference: Option<String>,
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data directory: {e}"))?;
    Ok(dir.join(CONFIG_FILE))
}

fn read_config(app: &AppHandle) -> Result<OnePasswordConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(OnePasswordConfig::default());
    }
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("could not read {CONFIG_FILE}: {e}"))?;
    // A hand-corrupted config shouldn't lock the user out of the app — fall
    // back to "not configured" and let them enter the passphrase manually.
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// Rejects anything that isn't a well-formed `op://vault/item/field`
/// reference before we ever hand it to a subprocess. The value reaches this
/// function from user input, so "it looked fine" isn't good enough — a bare
/// word here would be passed to `op` as an argument and produce a confusing
/// failure at unlock time instead of at configuration time.
fn validate_reference(reference: &str) -> Result<(), String> {
    let Some(rest) = reference.strip_prefix("op://") else {
        return Err("a 1Password reference must start with \"op://\".".to_string());
    };
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 3 {
        return Err(
            "a 1Password reference looks like op://Vault/Item/password — vault, item and field \
             are all required."
                .to_string(),
        );
    }
    Ok(())
}

/// Locates the `op` binary.
///
/// An app launched from Finder inherits a minimal `PATH` (roughly
/// `/usr/bin:/bin:/usr/sbin:/sbin`) that contains neither Homebrew prefix,
/// so a plain `Command::new("op")` works under `tauri dev` and then fails
/// for the installed `.app`. The known install locations are checked first
/// and `PATH` is only the fallback.
fn op_binary() -> PathBuf {
    for candidate in ["/opt/homebrew/bin/op", "/usr/local/bin/op"] {
        let path = Path::new(candidate);
        if path.exists() {
            return path.to_path_buf();
        }
    }
    PathBuf::from("op")
}

/// Fetches the passphrase from 1Password. The 1Password desktop app handles
/// the Touch ID prompt; Scrat never sees or stores the biometric result.
fn read_secret(reference: &str) -> Result<String, String> {
    validate_reference(reference)?;

    let output = Command::new(op_binary())
        .args(["read", reference, "--no-newline"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "the 1Password CLI (op) is not installed. Install it with \
                 `brew install 1password-cli`, then enable Settings → Developer → \
                 \"Integrate with 1Password CLI\" in the 1Password app."
                    .to_string()
            } else {
                format!("could not run the 1Password CLI: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            "1Password refused the request.".to_string()
        } else {
            format!("1Password: {detail}")
        });
    }

    let secret = String::from_utf8(output.stdout)
        .map_err(|_| "1Password returned a value that is not valid text.".to_string())?;
    // `--no-newline` should make this a no-op; kept so an older `op` that
    // ignores the flag doesn't silently append a byte to the passphrase.
    let secret = secret
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();

    if secret.is_empty() {
        return Err("that 1Password field is empty.".to_string());
    }
    Ok(secret)
}

/// Readable while the database is still locked — the unlock screen needs it
/// before there is any connection to read settings from. This is exactly why
/// the reference lives in a plain file rather than in the encrypted
/// `settings` table.
#[tauri::command]
pub fn get_1password_reference(app: AppHandle) -> Result<Option<String>, String> {
    Ok(read_config(&app)?.secret_reference)
}

/// Passing `None` turns the integration off and removes the stored pointer.
#[tauri::command]
pub fn set_1password_reference(app: AppHandle, reference: Option<String>) -> Result<(), String> {
    let reference = match reference {
        Some(r) if !r.trim().is_empty() => {
            let r = r.trim().to_string();
            validate_reference(&r)?;
            Some(r)
        }
        _ => None,
    };

    let path = config_path(&app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {CONFIG_FILE}: {e}"))?;
    }
    let config = OnePasswordConfig {
        secret_reference: reference,
    };
    let raw = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("could not write {CONFIG_FILE}: {e}"))
}

/// Confirms the stored value really is *this* database's passphrase, rather
/// than only confirming that `op` can read something.
///
/// Same validate-by-opening-then-dropping pattern as `import_database`: a
/// throwaway connection proves the passphrase before the user restarts and
/// discovers the mismatch at the unlock screen instead.
#[tauri::command]
pub fn test_1password_reference(app: AppHandle, reference: String) -> Result<(), String> {
    let secret = read_secret(&reference)?;
    let path = db_path(&app)?;
    if !scrat_infra_sqlite::database_exists(&path) {
        return Err("there is no database to test against yet".to_string());
    }
    drop(
        scrat_infra_sqlite::unlock_existing(&path, &secret).map_err(|e| match e {
            scrat_infra_sqlite::DbError::InvalidPassphrase => {
                "1Password returned a value, but it is not this database's passphrase.".to_string()
            }
            other => describe(other),
        })?,
    );
    Ok(())
}

/// Reads the passphrase from 1Password and unlocks in one step, so the
/// secret never reaches the frontend.
#[tauri::command]
pub fn unlock_with_1password(app: AppHandle, state: State<DbState>) -> Result<(), String> {
    let reference = read_config(&app)?
        .secret_reference
        .ok_or_else(|| "1Password unlock is not set up.".to_string())?;
    let secret = read_secret(&reference)?;
    let conn = scrat_infra_sqlite::unlock_existing(&db_path(&app)?, &secret).map_err(describe)?;
    *state.0.lock().unwrap() = Some(conn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_reference;

    #[test]
    fn accepts_a_full_reference() {
        assert!(validate_reference("op://Personal/Scrat/password").is_ok());
    }

    #[test]
    fn accepts_a_section_qualified_field() {
        assert!(validate_reference("op://Personal/Scrat/section/password").is_ok());
    }

    #[test]
    fn rejects_a_missing_scheme() {
        assert!(validate_reference("Personal/Scrat/password").is_err());
    }

    #[test]
    fn rejects_a_reference_without_a_field() {
        assert!(validate_reference("op://Personal/Scrat").is_err());
    }

    #[test]
    fn rejects_an_empty_reference() {
        assert!(validate_reference("").is_err());
    }
}
