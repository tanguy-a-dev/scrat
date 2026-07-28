use chrono::NaiveDate;
use scrat_domain::account::AccountId;
use scrat_domain::category::{Category, CategoryError, CategoryId, CategoryName};
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{
    AccountRepository, CategoryRepository, InsertOutcome, RepositoryError, TransactionRepository,
};
use scrat_domain::transaction::{SourceText, Transaction, TransactionError, TransactionId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Category(#[from] CategoryError),
    #[error("account not found")]
    AccountNotFound,
    #[error("category not found")]
    CategoryNotFound,
}

/// A single row a CSV importer has already parsed into a date/amount/source
/// triple, with the category it should be filed under already resolved.
/// Deliberately independent of any particular CSV crate's types —
/// `scrat-infra-csv` produces its own parsed rows and the Tauri command
/// layer maps them into this before calling [`TransactionService::import_transactions`].
#[derive(Debug, Clone)]
pub struct ImportRow {
    pub date: NaiveDate,
    pub amount_minor_units: i64,
    pub source: String,
    pub category_id: CategoryId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportOutcome {
    pub imported: usize,
    pub skipped_duplicates: usize,
}

/// Constructed fresh per request against live repository borrows — see
/// `AccountService` for why these borrow rather than own their repository.
pub struct TransactionService<'a> {
    transactions: &'a dyn TransactionRepository,
    accounts: &'a dyn AccountRepository,
    categories: &'a dyn CategoryRepository,
    currency: Currency,
}

impl<'a> TransactionService<'a> {
    pub fn new(
        transactions: &'a dyn TransactionRepository,
        accounts: &'a dyn AccountRepository,
        categories: &'a dyn CategoryRepository,
        currency: Currency,
    ) -> Self {
        Self {
            transactions,
            accounts,
            categories,
            currency,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_transaction(
        &self,
        date: NaiveDate,
        amount_minor_units: i64,
        source: &str,
        category_id: CategoryId,
        account_id: AccountId,
    ) -> Result<Transaction, ApplicationError> {
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;
        self.accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;

        let source = SourceText::new(source)?;
        let amount = Money::from_minor_units(amount_minor_units, self.currency.clone());
        let transaction = Transaction::new(
            TransactionId::new(),
            date,
            amount,
            source,
            category_id,
            account_id,
        )?;
        self.transactions.insert(&transaction)?;
        Ok(transaction)
    }

    pub fn delete_transaction(&self, id: TransactionId) -> Result<(), ApplicationError> {
        self.transactions.delete(id)?;
        Ok(())
    }

    /// Recategorizes an existing transaction, e.g. after the user notices it
    /// was filed under the wrong category.
    pub fn set_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), ApplicationError> {
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;
        self.transactions.update_category(id, category_id)?;
        Ok(())
    }

    pub fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, ApplicationError> {
        Ok(self.transactions.list_in_range(start, end)?)
    }

    /// Imports a batch of already-parsed rows (from a CSV, say) into the
    /// given account, each under its own already-resolved category, skipping
    /// any row whose dedup key already exists rather than erroring — makes
    /// re-importing the same file (or an overlapping date range) idempotent.
    pub fn import_transactions(
        &self,
        rows: &[ImportRow],
        account_id: AccountId,
    ) -> Result<ImportOutcome, ApplicationError> {
        self.accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;

        let mut outcome = ImportOutcome::default();
        for row in rows {
            self.categories
                .find_by_id(row.category_id)?
                .ok_or(ApplicationError::CategoryNotFound)?;
            let source = SourceText::new(&row.source)?;
            let amount = Money::from_minor_units(row.amount_minor_units, self.currency.clone());
            let transaction = Transaction::new(
                TransactionId::new(),
                row.date,
                amount,
                source,
                row.category_id,
                account_id,
            )?;
            match self.transactions.insert_or_skip(&transaction)? {
                InsertOutcome::Inserted => outcome.imported += 1,
                InsertOutcome::DuplicateSkipped => outcome.skipped_duplicates += 1,
            }
        }
        Ok(outcome)
    }

    /// Finds the account whose source-pattern list matches the given raw
    /// bank source text, if any — used to suggest (not force) an account
    /// while the user is filling in a transaction's source field.
    pub fn find_account_by_source(
        &self,
        source: &str,
    ) -> Result<Option<AccountId>, ApplicationError> {
        Ok(self
            .accounts
            .list_all()?
            .into_iter()
            .find(|a| a.matches_source(source))
            .map(|a| a.id()))
    }

    /// Finds an existing top-level or subcategory matching `name`
    /// (case-insensitive), or creates a new top-level category for it —
    /// used to honor a CSV's own Category column during import rather than
    /// forcing every row into one chosen category.
    pub fn get_or_create_category_by_name(
        &self,
        name: &str,
    ) -> Result<CategoryId, ApplicationError> {
        let trimmed = name.trim();
        if let Some(existing) = self
            .categories
            .list_all()?
            .into_iter()
            .find(|c| c.name().as_str().eq_ignore_ascii_case(trimmed))
        {
            return Ok(existing.id());
        }
        let category = Category::new(CategoryId::new(), CategoryName::new(trimmed)?, None)?;
        self.categories.insert(&category)?;
        Ok(category.id())
    }

    /// Local frequency lookup, no ML/network: finds past transactions that
    /// share a significant word with `source`, and suggests whichever
    /// category is most common among them.
    pub fn suggest_category_for_source(
        &self,
        source: &str,
    ) -> Result<Option<CategoryId>, ApplicationError> {
        let query_tokens = tokenize(source);
        if query_tokens.is_empty() {
            return Ok(None);
        }

        let all = self
            .transactions
            .list_in_range(NaiveDate::MIN, NaiveDate::MAX)?;

        let mut counts: std::collections::HashMap<CategoryId, usize> =
            std::collections::HashMap::new();
        for t in &all {
            let candidate_tokens = tokenize(t.source().as_str());
            if query_tokens.iter().any(|qt| candidate_tokens.contains(qt)) {
                *counts.entry(t.category_id()).or_insert(0) += 1;
            }
        }

        Ok(counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(id, _)| id))
    }
}

/// Lowercases and splits on non-alphanumeric boundaries, dropping short
/// tokens (numbers, single letters) that are more noise than signal for a
/// source-text match.
fn tokenize(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrat_domain::account::{Account, AccountName};
    use scrat_domain::category::{Category, CategoryName};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAccountRepository {
        accounts: Mutex<Vec<Account>>,
    }

    impl AccountRepository for FakeAccountRepository {
        fn insert(&self, account: &Account) -> Result<(), RepositoryError> {
            self.accounts.lock().unwrap().push(account.clone());
            Ok(())
        }
        fn update(&self, _account: &Account) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn delete(&self, _id: AccountId) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Ok(self
                .accounts
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.id() == id)
                .cloned())
        }
        fn list_all(&self) -> Result<Vec<Account>, RepositoryError> {
            Ok(self.accounts.lock().unwrap().clone())
        }
        fn transaction_count(&self, _id: AccountId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
        fn sum_transactions_minor_units(&self, _id: AccountId) -> Result<i64, RepositoryError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct FakeCategoryRepository {
        categories: Mutex<Vec<Category>>,
    }

    impl CategoryRepository for FakeCategoryRepository {
        fn insert(&self, category: &Category) -> Result<(), RepositoryError> {
            self.categories.lock().unwrap().push(category.clone());
            Ok(())
        }
        fn update(&self, _category: &Category) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn delete(&self, _id: CategoryId) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn find_by_id(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
            Ok(self
                .categories
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id() == id)
                .cloned())
        }
        fn list_all(&self) -> Result<Vec<Category>, RepositoryError> {
            Ok(self.categories.lock().unwrap().clone())
        }
        fn reassign_children(
            &self,
            _from: CategoryId,
            _to: Option<CategoryId>,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn reassign_transactions(
            &self,
            _from: CategoryId,
            _to: CategoryId,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn transaction_count(&self, _id: CategoryId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct FakeTransactionRepository {
        transactions: Mutex<Vec<Transaction>>,
    }

    impl TransactionRepository for FakeTransactionRepository {
        fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError> {
            self.transactions.lock().unwrap().push(transaction.clone());
            Ok(())
        }
        fn insert_or_skip(
            &self,
            transaction: &Transaction,
        ) -> Result<scrat_domain::ports::InsertOutcome, RepositoryError> {
            let mut transactions = self.transactions.lock().unwrap();
            if transactions
                .iter()
                .any(|t| t.dedup_key() == transaction.dedup_key())
            {
                return Ok(scrat_domain::ports::InsertOutcome::DuplicateSkipped);
            }
            transactions.push(transaction.clone());
            Ok(scrat_domain::ports::InsertOutcome::Inserted)
        }
        fn delete(&self, id: TransactionId) -> Result<(), RepositoryError> {
            self.transactions.lock().unwrap().retain(|t| t.id() != id);
            Ok(())
        }
        fn update_category(
            &self,
            id: TransactionId,
            category_id: CategoryId,
        ) -> Result<(), RepositoryError> {
            let mut transactions = self.transactions.lock().unwrap();
            if let Some(pos) = transactions.iter().position(|t| t.id() == id) {
                let existing = &transactions[pos];
                let updated = Transaction::new(
                    existing.id(),
                    existing.date(),
                    existing.amount().clone(),
                    existing.source().clone(),
                    category_id,
                    existing.account_id(),
                )
                .expect("recategorizing preserves validity");
                transactions[pos] = updated;
            }
            Ok(())
        }
        fn list_in_range(
            &self,
            start: NaiveDate,
            end: NaiveDate,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Ok(self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.date() >= start && t.date() <= end)
                .cloned()
                .collect())
        }
    }

    struct Fixture {
        transactions: FakeTransactionRepository,
        accounts: FakeAccountRepository,
        categories: FakeCategoryRepository,
        account_id: AccountId,
        category_id: CategoryId,
    }

    fn fixture() -> Fixture {
        let accounts = FakeAccountRepository::default();
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(Currency::new("USD").unwrap()),
        );
        let account_id = account.id();
        accounts.insert(&account).unwrap();

        let categories = FakeCategoryRepository::default();
        let category = Category::new(
            CategoryId::new(),
            CategoryName::new("Groceries").unwrap(),
            None,
        )
        .unwrap();
        let category_id = category.id();
        categories.insert(&category).unwrap();

        Fixture {
            transactions: FakeTransactionRepository::default(),
            accounts,
            categories,
            account_id,
            category_id,
        }
    }

    #[test]
    fn create_transaction_with_known_account_and_category_succeeds() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        assert_eq!(transaction.amount().minor_units(), -1_200);
    }

    #[test]
    fn create_transaction_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let result = service.create_transaction(
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            -1_200,
            "Whole Foods",
            CategoryId::new(),
            f.account_id,
        );

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn create_transaction_rejects_unknown_account() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let result = service.create_transaction(
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            -1_200,
            "Whole Foods",
            f.category_id,
            AccountId::new(),
        );

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn set_category_updates_transaction_to_new_known_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let dining = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&dining).unwrap();

        service.set_category(transaction.id(), dining.id()).unwrap();

        let stored = service
            .list_in_range(NaiveDate::MIN, NaiveDate::MAX)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].category_id(), dining.id());
    }

    #[test]
    fn set_category_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let result = service.set_category(transaction.id(), CategoryId::new());

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn list_in_range_excludes_transactions_outside_range() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "In range",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -500,
                "Out of range",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let results = service
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source().as_str(), "In range");
    }

    #[test]
    fn find_account_by_source_matches_saved_pattern() {
        let f = fixture();
        let mut account = f.accounts.accounts.lock().unwrap()[0].clone();
        account
            .add_source_pattern(scrat_domain::account::SourcePattern::new("whole foods").unwrap());
        f.accounts.accounts.lock().unwrap()[0] = account;
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let found = service.find_account_by_source("WHOLE FOODS #42").unwrap();

        assert_eq!(found, Some(f.account_id));
    }

    #[test]
    fn import_transactions_skips_rows_whose_dedup_key_already_exists() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            source: "Whole Foods".to_string(),
            category_id: f.category_id,
        };

        let first = service
            .import_transactions(&[row.clone(), row.clone()], f.account_id)
            .unwrap();

        assert_eq!(
            first,
            ImportOutcome {
                imported: 1,
                skipped_duplicates: 1
            }
        );

        // Re-importing the same "file" again is a no-op.
        let second = service.import_transactions(&[row], f.account_id).unwrap();
        assert_eq!(
            second,
            ImportOutcome {
                imported: 0,
                skipped_duplicates: 1
            }
        );
    }

    #[test]
    fn import_transactions_rejects_unknown_account() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            source: "Whole Foods".to_string(),
            category_id: f.category_id,
        };

        let result = service.import_transactions(&[row], AccountId::new());

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn import_transactions_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            source: "Whole Foods".to_string(),
            category_id: CategoryId::new(),
        };

        let result = service.import_transactions(&[row], f.account_id);

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn get_or_create_category_by_name_creates_new_top_level_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service.get_or_create_category_by_name("Books").unwrap();

        let stored = f.categories.find_by_id(id).unwrap().unwrap();
        assert_eq!(stored.name().as_str(), "Books");
        assert_eq!(stored.parent_id(), None);
    }

    #[test]
    fn get_or_create_category_by_name_matches_existing_case_insensitively() {
        let f = fixture(); // fixture already has a "Groceries" category
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service.get_or_create_category_by_name("groceries").unwrap();

        assert_eq!(id, f.category_id);
        assert_eq!(f.categories.categories.lock().unwrap().len(), 1); // no duplicate created
    }

    #[test]
    fn suggest_category_for_source_returns_most_common_category_among_matches() {
        let f = fixture();
        let entertainment = Category::new(
            CategoryId::new(),
            CategoryName::new("Entertainment").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&entertainment).unwrap();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        // Two past Netflix charges were (correctly) filed as Entertainment,
        // one was miscategorized as Groceries — Entertainment should win.
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_500,
                "NETFLIX.COM",
                entertainment.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                -1_500,
                "Netflix Subscription",
                entertainment.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -1_500,
                "netflix oops",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let suggestion = service.suggest_category_for_source("NETFLIX.COM").unwrap();

        assert_eq!(suggestion, Some(entertainment.id()));
    }

    #[test]
    fn suggest_category_for_source_returns_none_when_no_past_matches() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let suggestion = service
            .suggest_category_for_source("Totally Unseen Merchant")
            .unwrap();

        assert_eq!(suggestion, None);
    }
}
