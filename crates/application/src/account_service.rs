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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountWithBalance {
    pub account: Account,
    pub balance: Money,
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

    pub fn create_account(
        &self,
        name: &str,
        opening_balance_minor_units: i64,
    ) -> Result<Account, ApplicationError> {
        let name = AccountName::new(name)?;
        let opening_balance =
            Money::from_minor_units(opening_balance_minor_units, self.currency.clone());
        let account = Account::new(AccountId::new(), name, opening_balance);
        self.repo.insert(&account)?;
        Ok(account)
    }

    pub fn rename_account(&self, id: AccountId, new_name: &str) -> Result<(), ApplicationError> {
        let mut account = self.get(id)?;
        account.rename(AccountName::new(new_name)?);
        self.repo.update(&account)?;
        Ok(())
    }

    pub fn set_opening_balance(
        &self,
        id: AccountId,
        minor_units: i64,
    ) -> Result<(), ApplicationError> {
        let mut account = self.get(id)?;
        account.set_opening_balance(Money::from_minor_units(minor_units, self.currency.clone()));
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
                let balance = account
                    .opening_balance()
                    .add(&Money::from_minor_units(ledger_sum, self.currency.clone()))?;
                Ok(AccountWithBalance { account, balance })
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

        fn sum_transactions_minor_units(&self, _id: AccountId) -> Result<i64, RepositoryError> {
            Ok(0)
        }
    }

    #[test]
    fn create_account_with_valid_name_succeeds() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let account = service.create_account("Checking", 10_000).unwrap();

        assert_eq!(account.name().as_str(), "Checking");
        assert_eq!(account.opening_balance().minor_units(), 10_000);
    }

    #[test]
    fn create_account_with_blank_name_fails() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());

        let result = service.create_account("   ", 0);

        assert!(matches!(
            result,
            Err(ApplicationError::Account(AccountError::EmptyName))
        ));
    }

    #[test]
    fn delete_account_returns_error_when_transactions_reference_account() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        let account = service.create_account("Checking", 0).unwrap();
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
        let account = service.create_account("Checking", 0).unwrap();

        service.delete_account(account.id()).unwrap();

        assert!(repo.find_by_id(account.id()).unwrap().is_none());
    }

    #[test]
    fn list_accounts_with_balance_combines_opening_balance_and_ledger_sum() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        service.create_account("Checking", 5_000).unwrap();

        let accounts = service.list_accounts_with_balance().unwrap();

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].balance.minor_units(), 5_000);
    }

    #[test]
    fn total_available_sums_all_accounts() {
        let repo = FakeAccountRepository::default();
        let service = AccountService::new(&repo, Currency::new("USD").unwrap());
        service.create_account("Checking", 5_000).unwrap();
        service.create_account("Savings", 2_000).unwrap();

        let accounts = service.list_accounts_with_balance().unwrap();
        let total = total_available(&accounts).unwrap();

        assert_eq!(total.minor_units(), 7_000);
    }
}
