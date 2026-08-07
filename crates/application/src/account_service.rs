use scrat_domain::account::{Account, AccountError, AccountId, AccountName, DescriptionPattern};
use scrat_domain::money::{Currency, Money, MoneyError};
use scrat_domain::ports::{AccountRepository, RepositoryError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Money(#[from] MoneyError),
    #[error("account not found")]
    AccountNotFound,
    #[error("account still has {0} transaction(s); reassign or delete them first")]
    HasTransactions(u64),
    #[error("that balance is too large to work a starting point back from")]
    BalanceOutOfRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountWithBalance {
    pub account: Account,
    pub balance: Money,
    /// How many transactions the account holds. Carried alongside the
    /// balance because an unestablished starting point only actually
    /// misstates a balance once there are transactions to anchor — a fresh
    /// empty account is at zero either way, and shouldn't be flagged.
    pub transaction_count: u64,
}

/// Constructed fresh per request against a live repository borrow — the
/// repository's lifetime is tied to a locked database connection (e.g. a
/// `MutexGuard` held for the duration of a single Tauri command), so this
/// service borrows rather than owns it.
pub struct AccountService<'a> {
    repo: &'a dyn AccountRepository,
    currency: Currency,
}

impl<'a> AccountService<'a> {
    pub fn new(repo: &'a dyn AccountRepository, currency: Currency) -> Self {
        Self { repo, currency }
    }

    /// Creates an account with no starting point yet.
    ///
    /// Deliberately doesn't accept one: at creation time the ledger is empty,
    /// so the only honest answer would be today's balance — but the usual
    /// next step is importing months of history, which moves the starting
    /// point back to before the first imported row. Whatever the user typed
    /// here would be wrong the moment they imported. The anchor is
    /// established afterwards instead, by [`Self::establish_opening_balance`],
    /// back-solved from a balance they can actually read off their bank.
    pub fn create_account(&self, name: &str) -> Result<Account, ApplicationError> {
        let name = AccountName::new(name)?;
        let account = Account::without_opening_balance(AccountId::new(), name);
        self.repo.insert(&account)?;
        Ok(account)
    }

    pub fn rename_account(&self, id: AccountId, new_name: &str) -> Result<(), ApplicationError> {
        let mut account = self.get(id)?;
        account.rename(AccountName::new(new_name)?);
        self.repo.update(&account)?;
        Ok(())
    }

    /// Establishes the starting point from a balance the user can actually
    /// see, by working backwards through the ledger:
    /// `opening = observed - SUM(transactions)`.
    ///
    /// This is the only way the anchor gets set in practice, because it asks
    /// for the one number a user can produce. Note what it does *not* do:
    /// unlike [`TransactionService::reconcile_account`], it writes no ledger
    /// entry. That's the whole distinction between the two. This says "my
    /// records don't reach back far enough", and shifting the anchor makes
    /// every past balance correct at once; reconciling says "something
    /// happened since that I never imported", which is a real event on a real
    /// date and has to be posted as one. Using either for the other's job
    /// silently falsifies history — this one by back-dating money the account
    /// didn't have, that one by leaving every historical balance wrong.
    ///
    /// [`TransactionService::reconcile_account`]: crate::transaction_service::TransactionService::reconcile_account
    pub fn establish_opening_balance(
        &self,
        id: AccountId,
        observed_balance_minor_units: i64,
    ) -> Result<(), ApplicationError> {
        let account = self.get(id)?;
        let ledger_sum = self.repo.sum_transactions_minor_units(id)?;
        // Typed in by hand against a bank statement, so an extra digit or a
        // pasted account number must fail loudly rather than wrap around.
        let opening = observed_balance_minor_units
            .checked_sub(ledger_sum)
            .ok_or(ApplicationError::BalanceOutOfRange)?;
        let mut account = account;
        account.set_opening_balance(Money::from_minor_units(opening, self.currency.clone()));
        self.repo.update(&account)?;
        Ok(())
    }

    pub fn add_description_pattern(
        &self,
        id: AccountId,
        pattern: &str,
    ) -> Result<(), ApplicationError> {
        let mut account = self.get(id)?;
        account.add_description_pattern(DescriptionPattern::new(pattern)?);
        self.repo.update(&account)?;
        Ok(())
    }

    pub fn remove_description_pattern(
        &self,
        id: AccountId,
        pattern: &str,
    ) -> Result<(), ApplicationError> {
        let mut account = self.get(id)?;
        account.remove_description_pattern(&DescriptionPattern::new(pattern)?);
        self.repo.update(&account)?;
        Ok(())
    }

    /// Hard-deletes the account, unless it still has transactions — a
    /// finance app must never silently orphan or cascade-delete ledger
    /// history, so callers get an explicit error instead.
    pub fn delete_account(&self, id: AccountId) -> Result<(), ApplicationError> {
        let count = self.repo.transaction_count(id)?;
        if count > 0 {
            return Err(ApplicationError::HasTransactions(count));
        }
        self.repo.delete(id)?;
        Ok(())
    }

    pub fn list_accounts_with_balance(&self) -> Result<Vec<AccountWithBalance>, ApplicationError> {
        self.repo
            .list_all()?
            .into_iter()
            .map(|account| {
                let ledger_sum = self.repo.sum_transactions_minor_units(account.id())?;
                let transaction_count = self.repo.transaction_count(account.id())?;
                let balance = Money::from_minor_units(
                    account.opening_balance_minor_units(),
                    self.currency.clone(),
                )
                .add(&Money::from_minor_units(ledger_sum, self.currency.clone()))?;
                Ok(AccountWithBalance {
                    account,
                    balance,
                    transaction_count,
                })
            })
            .collect()
    }

    fn get(&self, id: AccountId) -> Result<Account, ApplicationError> {
        self.repo
            .find_by_id(id)?
            .ok_or(ApplicationError::AccountNotFound)
    }
}

/// Overview's "total available" — the sum of every account's balance.
pub fn total_available(accounts: &[AccountWithBalance]) -> Option<Money> {
    accounts
        .iter()
        .try_fold(None::<Money>, |acc, a| {
            let sum = match acc {
                Some(running) => running.add(&a.balance).ok()?,
                None => a.balance.clone(),
            };
            Some(Some(sum))
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAccountRepository {
        accounts: Mutex<Vec<Account>>,
        transaction_counts: Mutex<std::collections::HashMap<AccountId, u64>>,
        ledger_sums: Mutex<std::collections::HashMap<AccountId, i64>>,
    }

    impl AccountRepository for FakeAccountRepository {
        fn insert(&self, account: &Account) -> Result<(), RepositoryError> {
            self.accounts.lock().unwrap().push(account.clone());
            Ok(())
        }

        fn update(&self, account: &Account) -> Result<(), RepositoryError> {
            let mut accounts = self.accounts.lock().unwrap();
            let existing = accounts
                .iter_mut()
                .find(|a| a.id() == account.id())
                .expect("account must exist");
            *existing = account.clone();
            Ok(())
        }

        fn delete(&self, id: AccountId) -> Result<(), RepositoryError> {
            self.accounts.lock().unwrap().retain(|a| a.id() != id);
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

        fn transaction_count(&self, id: AccountId) -> Result<u64, RepositoryError> {
            Ok(*self
                .transaction_counts
                .lock()
                .unwrap()
                .get(&id)
                .unwrap_or(&0))
        }

        fn sum_transactions_minor_units(&self, id: AccountId) -> Result<i64, RepositoryError> {
            Ok(*self.ledger_sums.lock().unwrap().get(&id).unwrap_or(&0))
        }
    }

    #[test]
    fn create_account_with_valid_name_succeeds() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let account = service.create_account("Checking").unwrap();

        assert_eq!(account.name().as_str(), "Checking");
    }

    /// Creation must not invent a starting point. The account is unanchored
    /// until the user gives a balance to work back from — see
    /// [`AccountService::create_account`].
    #[test]
    fn create_account_leaves_the_starting_point_unestablished() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let account = service.create_account("Checking").unwrap();

        assert!(!account.is_opening_balance_set());
    }

    #[test]
    fn create_account_with_blank_name_fails() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let result = service.create_account("   ");

        assert!(matches!(
            result,
            Err(ApplicationError::Account(AccountError::EmptyName))
        ));
    }

    /// The core arithmetic: the user reports what their bank shows, and the
    /// anchor is whatever makes the imported ledger add up to it. Here 250.00
    /// of imported history against an observed 1,000.00 means the account
    /// must have held 750.00 before that history began.
    #[test]
    fn establish_opening_balance_works_backwards_from_the_observed_balance() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums
            .lock()
            .unwrap()
            .insert(account.id(), 25_000);

        service
            .establish_opening_balance(account.id(), 100_000)
            .unwrap();

        let stored = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(stored.opening_balance_minor_units(), 75_000);
        assert!(stored.is_opening_balance_set());
    }

    /// Answering "it started at zero" is an answer. The stored amount is
    /// indistinguishable from the unset default, so only the flag keeps the
    /// app from asking again forever.
    #[test]
    fn establish_opening_balance_marks_a_zero_anchor_as_established() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums
            .lock()
            .unwrap()
            .insert(account.id(), 25_000);

        service
            .establish_opening_balance(account.id(), 25_000)
            .unwrap();

        let stored = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(stored.opening_balance_minor_units(), 0);
        assert!(stored.is_opening_balance_set());
    }

    /// Re-running against an account that already has an anchor overwrites
    /// it, computed fresh from the current ledger rather than adjusted
    /// relative to the old value. This is the only way to correct a mistyped
    /// starting point — nothing else can move the anchor — so it must stay an
    /// overwrite and not start refusing once one exists.
    #[test]
    fn establish_opening_balance_overwrites_an_anchor_that_was_already_set() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums
            .lock()
            .unwrap()
            .insert(account.id(), 25_000);
        // A fat-fingered first attempt: an extra zero on the observed balance.
        service
            .establish_opening_balance(account.id(), 1_000_000)
            .unwrap();

        service
            .establish_opening_balance(account.id(), 100_000)
            .unwrap();

        let stored = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(stored.opening_balance_minor_units(), 75_000);
        assert!(stored.is_opening_balance_set());
    }

    /// Hand-typed against a bank statement, so a pasted account number must
    /// fail loudly rather than wrap `i64` into a plausible-looking anchor.
    #[test]
    fn establish_opening_balance_rejects_an_observed_balance_that_would_overflow() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums.lock().unwrap().insert(account.id(), -1);

        let result = service.establish_opening_balance(account.id(), i64::MAX);

        assert!(matches!(result, Err(ApplicationError::BalanceOutOfRange)));
    }

    #[test]
    fn establish_opening_balance_rejects_an_unknown_account() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let result = service.establish_opening_balance(AccountId::new(), 1_000);

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn delete_account_returns_error_when_transactions_reference_account() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.transaction_counts
            .lock()
            .unwrap()
            .insert(account.id(), 3);

        let result = service.delete_account(account.id());

        assert!(matches!(result, Err(ApplicationError::HasTransactions(3))));
    }

    #[test]
    fn delete_account_succeeds_when_no_transactions() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        service.delete_account(account.id()).unwrap();

        assert!(repo.find_by_id(account.id()).unwrap().is_none());
    }

    #[test]
    fn list_accounts_with_balance_combines_opening_balance_and_ledger_sum() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums.lock().unwrap().insert(account.id(), 2_000);
        service
            .establish_opening_balance(account.id(), 5_000)
            .unwrap();

        let accounts = service.list_accounts_with_balance().unwrap();

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].balance.minor_units(), 5_000);
    }

    /// An unanchored account still reports a balance — the ledger sum alone.
    /// It's the best available answer, and `is_opening_balance_set` is what
    /// tells the UI to present it as provisional rather than suppress it.
    #[test]
    fn list_accounts_with_balance_falls_back_to_the_ledger_sum_when_unanchored() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        repo.ledger_sums.lock().unwrap().insert(account.id(), 2_000);
        repo.transaction_counts
            .lock()
            .unwrap()
            .insert(account.id(), 4);

        let accounts = service.list_accounts_with_balance().unwrap();

        assert_eq!(accounts[0].balance.minor_units(), 2_000);
        assert!(!accounts[0].account.is_opening_balance_set());
        assert_eq!(accounts[0].transaction_count, 4);
    }

    #[test]
    fn total_available_sums_all_accounts() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let checking = service.create_account("Checking").unwrap();
        let savings = service.create_account("Savings").unwrap();
        service
            .establish_opening_balance(checking.id(), 5_000)
            .unwrap();
        service
            .establish_opening_balance(savings.id(), 2_000)
            .unwrap();

        let accounts = service.list_accounts_with_balance().unwrap();
        let total = total_available(&accounts).unwrap();

        assert_eq!(total.minor_units(), 7_000);
    }

    fn patterns_of(repo: &FakeAccountRepository, id: AccountId) -> Vec<String> {
        repo.find_by_id(id)
            .unwrap()
            .unwrap()
            .description_patterns()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    #[test]
    fn add_description_pattern_persists_it_on_the_account() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        service
            .add_description_pattern(account.id(), "Whole Foods")
            .unwrap();

        assert_eq!(patterns_of(&repo, account.id()), ["whole foods"]);
    }

    /// The pattern is normalized on the way in, so what lands in the
    /// repository is the lowercased, trimmed form — not whatever spacing the
    /// user happened to type.
    #[test]
    fn add_description_pattern_stores_the_normalized_form() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        service
            .add_description_pattern(account.id(), "  Whole FOODS  ")
            .unwrap();

        assert_eq!(patterns_of(&repo, account.id()), ["whole foods"]);
    }

    /// Removal normalizes its argument the same way adding does. It has to:
    /// the stored form is already lowercased and trimmed, so comparing the
    /// raw input against it would make removing "Whole Foods" a silent no-op
    /// against a pattern stored as "whole foods" — the row would stay on
    /// screen with nothing to explain why the delete did nothing.
    #[test]
    fn remove_description_pattern_matches_regardless_of_how_it_is_typed() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        service
            .add_description_pattern(account.id(), "whole foods")
            .unwrap();

        service
            .remove_description_pattern(account.id(), "  Whole FOODS ")
            .unwrap();

        assert!(patterns_of(&repo, account.id()).is_empty());
    }

    #[test]
    fn adding_the_same_pattern_twice_keeps_one_copy() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        service
            .add_description_pattern(account.id(), "Whole Foods")
            .unwrap();
        service
            .add_description_pattern(account.id(), "WHOLE FOODS")
            .unwrap();

        assert_eq!(patterns_of(&repo, account.id()), ["whole foods"]);
    }

    #[test]
    fn removing_a_pattern_that_was_never_added_is_not_an_error() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();
        service
            .add_description_pattern(account.id(), "whole foods")
            .unwrap();

        service
            .remove_description_pattern(account.id(), "trader joes")
            .unwrap();

        assert_eq!(patterns_of(&repo, account.id()), ["whole foods"]);
    }

    /// An all-whitespace pattern normalizes to the empty string, which would
    /// match every description ever imported and quietly claim the whole
    /// ledger for one account. It's rejected at the value object.
    #[test]
    fn a_blank_description_pattern_is_rejected() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        let error = service
            .add_description_pattern(account.id(), "   ")
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Account(AccountError::EmptyPattern)
        ));
        assert!(patterns_of(&repo, account.id()).is_empty());
    }

    #[test]
    fn adding_a_pattern_to_a_missing_account_reports_it_as_missing() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let error = service
            .add_description_pattern(AccountId::new(), "whole foods")
            .unwrap_err();

        assert!(matches!(error, ApplicationError::AccountNotFound));
    }

    #[test]
    fn removing_a_pattern_from_a_missing_account_reports_it_as_missing() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let error = service
            .remove_description_pattern(AccountId::new(), "whole foods")
            .unwrap_err();

        assert!(matches!(error, ApplicationError::AccountNotFound));
    }

    #[test]
    fn rename_account_persists_the_new_name() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        service.rename_account(account.id(), "Everyday").unwrap();

        let reloaded = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(reloaded.name().as_str(), "Everyday");
    }

    /// A rejected rename must leave the stored name alone rather than
    /// half-applying — validation happens before the repository is touched.
    #[test]
    fn rename_account_to_a_blank_name_is_rejected_and_changes_nothing() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking").unwrap();

        assert!(service.rename_account(account.id(), "   ").is_err());

        let reloaded = repo.find_by_id(account.id()).unwrap().unwrap();
        assert_eq!(reloaded.name().as_str(), "Checking");
    }

    #[test]
    fn renaming_a_missing_account_reports_it_as_missing() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let error = service
            .rename_account(AccountId::new(), "Everyday")
            .unwrap_err();

        assert!(matches!(error, ApplicationError::AccountNotFound));
    }
}
