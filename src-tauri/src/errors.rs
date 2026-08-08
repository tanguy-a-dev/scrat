//! The error shape every `#[tauri::command]` returns.
//!
//! Commands used to return `Result<T, String>` carrying English prose, which
//! the frontend showed verbatim in a toast. That stopped working the moment
//! the interface became translatable: the wording has to come from the same
//! dictionary as every other string the user reads, and Rust is the wrong
//! place to keep a second copy of it in two languages.
//!
//! So the wire now carries a **stable machine code** plus whatever values the
//! sentence needs interpolated, and `frontend/src/lib/i18n.svelte.ts` owns the
//! wording. A code is part of the IPC contract exactly like a command name —
//! renaming one silently degrades every message that used it to the generic
//! fallback, so treat the strings below as fixed.
//!
//! ## Why there is still a `detail` on some codes
//!
//! Not every failure is a sentence a user can act on. A SQLCipher error or an
//! I/O error is a diagnostic, and inventing a translated euphemism for it
//! would cost the one person who could use it — whoever is debugging — the
//! only useful part. Those codes carry the raw message as `detail`, and the
//! frontend renders it inside a translated frame ("Database error: {detail}").
//! Codes that describe something the user did wrong never need one.

use std::collections::BTreeMap;

use scrat_domain::account::AccountError;
use scrat_domain::category::CategoryError;
use scrat_domain::money::MoneyError;
use scrat_domain::ports::RepositoryError;
use scrat_domain::transaction::TransactionError;
use scrat_domain::transfer_rule::TransferRuleError;
use scrat_infra_sqlite::DbError;
use serde::Serialize;

/// A failure the frontend can render in the user's language.
///
/// `params` is a flat string map rather than a typed payload per code: it
/// crosses into TypeScript as a plain object either way, and a typed variant
/// per code would mean maintaining the same shape three times (here, in the
/// DTO, and in the dictionary) for no additional safety on the far side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppError {
    pub code: &'static str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<&'static str, String>,
}

impl AppError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
        }
    }

    pub fn with(mut self, key: &'static str, value: impl ToString) -> Self {
        self.params.insert(key, value.to_string());
        self
    }

    /// The database is not open. Every command that touches data starts here,
    /// which is why it has a constructor of its own.
    pub fn db_locked() -> Self {
        Self::new(codes::DB_LOCKED)
    }
}

impl std::fmt::Display for AppError {
    /// Only for `Debug`/logging and the few call sites still threading a
    /// `String`. Users never see this — they see the dictionary's rendering
    /// of `code`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;
        if !self.params.is_empty() {
            write!(f, " {:?}", self.params)?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

/// Every code the backend can emit, in one place so the frontend dictionary
/// can be checked against it. `frontend/src/lib/i18n.svelte.ts` must have an
/// entry for each; `ipc-contract.test.ts` is what enforces that.
pub mod codes {
    // Session and storage
    pub const DB_LOCKED: &str = "db_locked";
    pub const DB_ALREADY_EXISTS: &str = "db_already_exists";
    pub const INCORRECT_PASSPHRASE: &str = "incorrect_passphrase";
    pub const PASSPHRASE_EMPTY: &str = "passphrase_empty";
    pub const PASSPHRASE_TOO_SHORT: &str = "passphrase_too_short";
    pub const APP_DATA_DIR_UNAVAILABLE: &str = "app_data_dir_unavailable";
    pub const DATABASE_ERROR: &str = "database_error";
    pub const FILESYSTEM_ERROR: &str = "filesystem_error";

    // Lookups
    pub const ACCOUNT_NOT_FOUND: &str = "account_not_found";
    pub const CATEGORY_NOT_FOUND: &str = "category_not_found";

    // Domain validation
    pub const ACCOUNT_NAME_EMPTY: &str = "account_name_empty";
    pub const ACCOUNT_NAME_TOO_LONG: &str = "account_name_too_long";
    pub const ACCOUNT_PATTERN_EMPTY: &str = "account_pattern_empty";
    pub const CATEGORY_NAME_EMPTY: &str = "category_name_empty";
    pub const CATEGORY_NAME_TOO_LONG: &str = "category_name_too_long";
    pub const CATEGORY_SELF_PARENT: &str = "category_self_parent";
    pub const CATEGORY_UNKNOWN_ICON: &str = "category_unknown_icon";
    pub const SUBCATEGORY_CANNOT_HAVE_ICON: &str = "subcategory_cannot_have_icon";
    pub const CATEGORY_SEED_KEY_EMPTY: &str = "category_seed_key_empty";
    pub const AMOUNT_ZERO: &str = "amount_zero";
    pub const DESCRIPTION_EMPTY: &str = "description_empty";
    pub const DESCRIPTION_TOO_LONG: &str = "description_too_long";
    pub const TRANSFER_WITHOUT_GROUP: &str = "transfer_without_group";
    pub const GROUP_WITHOUT_TRANSFER_ROLE: &str = "group_without_transfer_role";
    pub const UNKNOWN_TRANSACTION_ROLE: &str = "unknown_transaction_role";
    pub const UNKNOWN_OPERATION_KIND: &str = "unknown_operation_kind";
    pub const INVALID_CURRENCY_CODE: &str = "invalid_currency_code";
    pub const CURRENCY_MISMATCH: &str = "currency_mismatch";
    pub const INVALID_ID: &str = "invalid_id";
    pub const INVALID_DATE: &str = "invalid_date";

    // Application rules
    pub const ACCOUNT_HAS_TRANSACTIONS: &str = "account_has_transactions";
    pub const CATEGORY_REQUIRES_REASSIGNMENT: &str = "category_requires_reassignment";
    pub const DEFAULT_CATEGORY_PROTECTED: &str = "default_category_protected";
    pub const PARENT_IS_SUBCATEGORY: &str = "parent_is_subcategory";
    pub const CATEGORY_HAS_SUBCATEGORIES: &str = "category_has_subcategories";
    pub const DUPLICATE_TRANSFER_RULE: &str = "duplicate_transfer_rule";
    pub const BALANCE_OUT_OF_RANGE: &str = "balance_out_of_range";
    pub const INVALID_REORDER: &str = "invalid_reorder";

    // Settings
    pub const UNSUPPORTED_LANGUAGE: &str = "unsupported_language";
    pub const AUTO_LOCK_INVALID: &str = "auto_lock_invalid";
    pub const AUTO_LOCK_STORED_INVALID: &str = "auto_lock_stored_invalid";

    // Backup, restore, import
    pub const NOTHING_TO_EXPORT: &str = "nothing_to_export";
    pub const IMPORT_FILE_MISSING: &str = "import_file_missing";
    pub const IMPORT_FINALIZE_FAILED: &str = "import_finalize_failed";
    pub const IMPORT_REOPEN_FAILED: &str = "import_reopen_failed";
    pub const CSV_FILE_TOO_LARGE: &str = "csv_file_too_large";
    pub const NO_DESTINATION_ACCOUNT: &str = "no_destination_account";
    pub const TOO_MANY_SELECTED: &str = "too_many_selected";
}

impl From<DbError> for AppError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::InvalidPassphrase => AppError::new(codes::INCORRECT_PASSPHRASE),
            DbError::EmptyPassphrase => AppError::new(codes::PASSPHRASE_EMPTY),
            DbError::AlreadyExists(_) => AppError::new(codes::DB_ALREADY_EXISTS),
            DbError::Sqlite(e) => AppError::new(codes::DATABASE_ERROR).with("detail", e),
            DbError::Io(e) => AppError::new(codes::FILESYSTEM_ERROR).with("detail", e),
            DbError::Repository(e) => AppError::new(codes::DATABASE_ERROR).with("detail", e),
        }
    }
}

impl From<RepositoryError> for AppError {
    fn from(err: RepositoryError) -> Self {
        AppError::new(codes::DATABASE_ERROR).with("detail", err)
    }
}

impl From<AccountError> for AppError {
    fn from(err: AccountError) -> Self {
        match err {
            AccountError::EmptyName => AppError::new(codes::ACCOUNT_NAME_EMPTY),
            AccountError::NameTooLong(max) => {
                AppError::new(codes::ACCOUNT_NAME_TOO_LONG).with("max", max)
            }
            AccountError::EmptyPattern => AppError::new(codes::ACCOUNT_PATTERN_EMPTY),
            AccountError::InvalidId(id) => AppError::new(codes::INVALID_ID).with("value", id),
        }
    }
}

impl From<CategoryError> for AppError {
    fn from(err: CategoryError) -> Self {
        match err {
            CategoryError::EmptyName => AppError::new(codes::CATEGORY_NAME_EMPTY),
            CategoryError::NameTooLong(max) => {
                AppError::new(codes::CATEGORY_NAME_TOO_LONG).with("max", max)
            }
            CategoryError::InvalidId(id) => AppError::new(codes::INVALID_ID).with("value", id),
            CategoryError::SelfParent => AppError::new(codes::CATEGORY_SELF_PARENT),
            CategoryError::UnknownIcon(icon) => {
                AppError::new(codes::CATEGORY_UNKNOWN_ICON).with("icon", icon)
            }
            CategoryError::SubcategoryCannotHaveIcon => {
                AppError::new(codes::SUBCATEGORY_CANNOT_HAVE_ICON)
            }
            CategoryError::EmptySeedKey => AppError::new(codes::CATEGORY_SEED_KEY_EMPTY),
        }
    }
}

impl From<TransactionError> for AppError {
    fn from(err: TransactionError) -> Self {
        match err {
            TransactionError::ZeroAmount => AppError::new(codes::AMOUNT_ZERO),
            TransactionError::EmptyDescription => AppError::new(codes::DESCRIPTION_EMPTY),
            TransactionError::DescriptionTooLong(max) => {
                AppError::new(codes::DESCRIPTION_TOO_LONG).with("max", max)
            }
            TransactionError::InvalidId(id) => AppError::new(codes::INVALID_ID).with("value", id),
            TransactionError::TransferWithoutGroup => AppError::new(codes::TRANSFER_WITHOUT_GROUP),
            TransactionError::GroupWithoutTransferRole => {
                AppError::new(codes::GROUP_WITHOUT_TRANSFER_ROLE)
            }
            TransactionError::UnknownRole(role) => {
                AppError::new(codes::UNKNOWN_TRANSACTION_ROLE).with("value", role)
            }
            TransactionError::UnknownOperationKind(kind) => {
                AppError::new(codes::UNKNOWN_OPERATION_KIND).with("value", kind)
            }
        }
    }
}

impl From<TransferRuleError> for AppError {
    fn from(err: TransferRuleError) -> Self {
        match err {
            TransferRuleError::InvalidId(id) => AppError::new(codes::INVALID_ID).with("value", id),
        }
    }
}

impl From<MoneyError> for AppError {
    fn from(err: MoneyError) -> Self {
        match err {
            MoneyError::InvalidCurrencyCode(code) => {
                AppError::new(codes::INVALID_CURRENCY_CODE).with("value", code)
            }
            MoneyError::CurrencyMismatch(left, right) => AppError::new(codes::CURRENCY_MISMATCH)
                .with("left", left)
                .with("right", right),
        }
    }
}

impl From<scrat_application::account_service::ApplicationError> for AppError {
    fn from(err: scrat_application::account_service::ApplicationError) -> Self {
        use scrat_application::account_service::ApplicationError as E;
        match err {
            E::Account(e) => e.into(),
            E::Repository(e) => e.into(),
            E::Money(e) => e.into(),
            E::AccountNotFound => AppError::new(codes::ACCOUNT_NOT_FOUND),
            E::HasTransactions(count) => {
                AppError::new(codes::ACCOUNT_HAS_TRANSACTIONS).with("count", count)
            }
            E::BalanceOutOfRange => AppError::new(codes::BALANCE_OUT_OF_RANGE),
            E::InvalidReorder => AppError::new(codes::INVALID_REORDER),
        }
    }
}

impl From<scrat_application::category_service::ApplicationError> for AppError {
    fn from(err: scrat_application::category_service::ApplicationError) -> Self {
        use scrat_application::category_service::ApplicationError as E;
        match err {
            E::Category(e) => e.into(),
            E::Repository(e) => e.into(),
            E::CategoryNotFound => AppError::new(codes::CATEGORY_NOT_FOUND),
            E::ParentIsSubcategory => AppError::new(codes::PARENT_IS_SUBCATEGORY),
            E::HasSubcategories => AppError::new(codes::CATEGORY_HAS_SUBCATEGORIES),
            E::RequiresReassignment(count) => {
                AppError::new(codes::CATEGORY_REQUIRES_REASSIGNMENT).with("count", count)
            }
            // The name is deliberately not passed through: the frontend knows
            // this category by its own translated label, and the English
            // constant would contradict what's on screen in French.
            E::DefaultCategoryProtected(_) => AppError::new(codes::DEFAULT_CATEGORY_PROTECTED),
        }
    }
}

impl From<scrat_application::transaction_service::ApplicationError> for AppError {
    fn from(err: scrat_application::transaction_service::ApplicationError) -> Self {
        use scrat_application::transaction_service::ApplicationError as E;
        match err {
            E::Transaction(e) => e.into(),
            E::Repository(e) => e.into(),
            E::Category(e) => e.into(),
            E::AccountNotFound => AppError::new(codes::ACCOUNT_NOT_FOUND),
            E::CategoryNotFound => AppError::new(codes::CATEGORY_NOT_FOUND),
            E::BalanceOutOfRange => AppError::new(codes::BALANCE_OUT_OF_RANGE),
        }
    }
}

impl From<scrat_application::transfer_rule_service::ApplicationError> for AppError {
    fn from(err: scrat_application::transfer_rule_service::ApplicationError) -> Self {
        use scrat_application::transfer_rule_service::ApplicationError as E;
        match err {
            E::Account(e) => e.into(),
            E::Repository(e) => e.into(),
            E::AccountNotFound => AppError::new(codes::ACCOUNT_NOT_FOUND),
            E::DuplicatePattern(pattern) => {
                AppError::new(codes::DUPLICATE_TRANSFER_RULE).with("pattern", pattern)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A code with no interpolation must serialize without a `params` key at
    /// all, not with an empty object — the frontend's message lookup treats a
    /// present-but-empty params bag and an absent one identically, but the
    /// smaller payload is the one the contract test asserts against.
    #[test]
    fn a_code_without_params_serializes_as_just_a_code() {
        let json = serde_json::to_string(&AppError::db_locked()).unwrap();
        assert_eq!(json, r#"{"code":"db_locked"}"#);
    }

    #[test]
    fn params_are_carried_as_strings_the_frontend_can_interpolate() {
        let error = AppError::new(codes::ACCOUNT_HAS_TRANSACTIONS).with("count", 12u64);
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(
            json,
            r#"{"code":"account_has_transactions","params":{"count":"12"}}"#
        );
    }

    /// The whole point of the layer: a domain error arrives as a code the
    /// dictionary can translate, not as the English sentence `thiserror`
    /// generated.
    #[test]
    fn domain_errors_become_codes_rather_than_english_prose() {
        let error: AppError = CategoryError::NameTooLong(100).into();
        assert_eq!(error.code, codes::CATEGORY_NAME_TOO_LONG);
        assert_eq!(error.params.get("max"), Some(&"100".to_string()));
    }

    /// A wrong passphrase is the one error a user meets before anything else
    /// works, and it must not be reported as a generic database failure.
    #[test]
    fn a_wrong_passphrase_keeps_its_own_code() {
        let error: AppError = DbError::InvalidPassphrase.into();
        assert_eq!(error.code, codes::INCORRECT_PASSPHRASE);
        assert!(error.params.is_empty(), "expected no interpolated params");
    }

    /// Diagnostics keep their detail — translating a SQLCipher message into a
    /// reassuring sentence would cost whoever is debugging the only useful
    /// part of it.
    #[test]
    fn diagnostic_errors_carry_their_detail_through() {
        let error: AppError = RepositoryError("no such column: seed_key".to_string()).into();
        assert_eq!(error.code, codes::DATABASE_ERROR);
        assert_eq!(
            error.params.get("detail"),
            Some(&"no such column: seed_key".to_string())
        );
    }

    /// The protected-category error deliberately drops the English name it
    /// carries: the frontend renders the category's own translated label.
    #[test]
    fn the_protected_category_error_does_not_leak_its_english_name() {
        use scrat_application::category_service::ApplicationError as E;
        let error: AppError = E::DefaultCategoryProtected("Uncategorized".to_string()).into();
        assert_eq!(error.code, codes::DEFAULT_CATEGORY_PROTECTED);
        assert!(error.params.is_empty(), "expected no interpolated params");
    }
}
