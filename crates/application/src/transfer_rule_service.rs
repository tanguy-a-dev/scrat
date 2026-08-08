use scrat_domain::account::{AccountError, AccountId, DescriptionPattern};
use scrat_domain::ports::{AccountRepository, RepositoryError, TransferRuleRepository};
use scrat_domain::transfer_rule::{TransferRule, TransferRuleId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("account not found")]
    AccountNotFound,
    #[error("a transfer rule for {0:?} already exists")]
    DuplicatePattern(String),
}

/// Constructed fresh per request against live repository borrows — see
/// `AccountService` for why these borrow rather than own their repositories.
pub struct TransferRuleService<'a> {
    rules: &'a dyn TransferRuleRepository,
    accounts: &'a dyn AccountRepository,
}

impl<'a> TransferRuleService<'a> {
    pub fn new(rules: &'a dyn TransferRuleRepository, accounts: &'a dyn AccountRepository) -> Self {
        Self { rules, accounts }
    }

    pub fn list_rules(&self) -> Result<Vec<TransferRule>, ApplicationError> {
        Ok(self.rules.list_all()?)
    }

    /// Rejects a pattern that another rule already claims. Two rules
    /// disagreeing about where the same description text sends money has no
    /// sensible resolution — whichever happened to be found first during
    /// import would win, which is a coin flip the user can't see or
    /// predict — so the conflict surfaces here instead.
    pub fn create_rule(
        &self,
        pattern: &str,
        counterpart_account_id: AccountId,
    ) -> Result<TransferRule, ApplicationError> {
        let pattern = DescriptionPattern::new(pattern)?;
        self.accounts
            .find_by_id(counterpart_account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;

        if self
            .rules
            .list_all()?
            .iter()
            .any(|existing| existing.pattern() == &pattern)
        {
            return Err(ApplicationError::DuplicatePattern(
                pattern.as_str().to_string(),
            ));
        }

        let rule = TransferRule::new(TransferRuleId::new(), pattern, counterpart_account_id);
        self.rules.insert(&rule)?;
        Ok(rule)
    }

    /// Deleting a rule stops *future* imports from being recognized as
    /// transfers; transfers already in the ledger are untouched. They are
    /// real recorded movements, not a cached consequence of the rule.
    pub fn delete_rule(&self, id: TransferRuleId) -> Result<(), ApplicationError> {
        self.rules.delete(id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrat_domain::account::{Account, AccountName};
    use scrat_domain::money::{Currency, Money};
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
        fn reorder(&self, _ordered_ids: &[AccountId]) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn transaction_count(&self, _id: AccountId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
        fn sum_transactions_minor_units(&self, _id: AccountId) -> Result<i64, RepositoryError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct FakeTransferRuleRepository {
        rules: Mutex<Vec<TransferRule>>,
    }

    impl TransferRuleRepository for FakeTransferRuleRepository {
        fn insert(&self, rule: &TransferRule) -> Result<(), RepositoryError> {
            self.rules.lock().unwrap().push(rule.clone());
            Ok(())
        }
        fn delete(&self, id: TransferRuleId) -> Result<(), RepositoryError> {
            self.rules.lock().unwrap().retain(|r| r.id() != id);
            Ok(())
        }
        fn list_all(&self) -> Result<Vec<TransferRule>, RepositoryError> {
            Ok(self.rules.lock().unwrap().clone())
        }
    }

    struct Fixture {
        rules: FakeTransferRuleRepository,
        accounts: FakeAccountRepository,
        counterpart_account_id: AccountId,
    }

    fn fixture() -> Fixture {
        let accounts = FakeAccountRepository::default();
        let account = Account::new(
            AccountId::new(),
            AccountName::new("N26").unwrap(),
            Money::zero(Currency::new("EUR").unwrap()),
        );
        let counterpart_account_id = account.id();
        accounts.insert(&account).unwrap();

        Fixture {
            rules: FakeTransferRuleRepository::default(),
            accounts,
            counterpart_account_id,
        }
    }

    #[test]
    fn create_rule_normalizes_the_pattern() {
        let f = fixture();
        let service = TransferRuleService::new(&f.rules, &f.accounts);

        let rule = service
            .create_rule("  VIREMENT N26  ", f.counterpart_account_id)
            .unwrap();

        assert_eq!(rule.pattern().as_str(), "virement n26");
        assert_eq!(rule.counterpart_account_id(), f.counterpart_account_id);
    }

    #[test]
    fn create_rule_rejects_an_unknown_counterpart_account() {
        let f = fixture();
        let service = TransferRuleService::new(&f.rules, &f.accounts);

        let result = service.create_rule("virement n26", AccountId::new());

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    /// Normalization happens before the duplicate check, so differing case
    /// or padding can't sneak a second rule past it.
    #[test]
    fn create_rule_rejects_a_pattern_another_rule_already_claims() {
        let f = fixture();
        let service = TransferRuleService::new(&f.rules, &f.accounts);
        service
            .create_rule("virement n26", f.counterpart_account_id)
            .unwrap();

        let result = service.create_rule("  VIREMENT N26 ", f.counterpart_account_id);

        assert!(
            matches!(result, Err(ApplicationError::DuplicatePattern(p)) if p == "virement n26")
        );
    }

    #[test]
    fn create_rule_rejects_an_empty_pattern() {
        let f = fixture();
        let service = TransferRuleService::new(&f.rules, &f.accounts);

        let result = service.create_rule("   ", f.counterpart_account_id);

        assert!(matches!(
            result,
            Err(ApplicationError::Account(AccountError::EmptyPattern))
        ));
    }

    #[test]
    fn delete_rule_removes_it_from_the_list() {
        let f = fixture();
        let service = TransferRuleService::new(&f.rules, &f.accounts);
        let rule = service
            .create_rule("virement n26", f.counterpart_account_id)
            .unwrap();

        service.delete_rule(rule.id()).unwrap();

        assert!(service.list_rules().unwrap().is_empty());
    }
}
