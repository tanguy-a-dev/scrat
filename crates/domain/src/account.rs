use thiserror::Error;
use uuid::Uuid;

use crate::money::Money;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AccountError {
    #[error("account name cannot be empty")]
    EmptyName,
    #[error("account name cannot be longer than {0} characters")]
    NameTooLong(usize),
    #[error("description pattern cannot be empty")]
    EmptyPattern,
    #[error("invalid id: {0}")]
    InvalidId(String),
}

const MAX_NAME_LEN: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountId(Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, AccountError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| AccountError::InvalidId(raw.to_string()))
    }

    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountName(String);

impl AccountName {
    pub fn new(raw: &str) -> Result<Self, AccountError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(AccountError::EmptyName);
        }
        if trimmed.chars().count() > MAX_NAME_LEN {
            return Err(AccountError::NameTooLong(MAX_NAME_LEN));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A normalized (trimmed, lowercased) substring pattern matched against a
/// transaction's raw description text to auto-assign it to an account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DescriptionPattern(String);

impl DescriptionPattern {
    pub fn new(raw: &str) -> Result<Self, AccountError> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(AccountError::EmptyPattern);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, description: &str) -> bool {
        description.to_lowercase().contains(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    id: AccountId,
    name: AccountName,
    /// Where the account stood before the first transaction on record.
    ///
    /// `None` means nobody has said yet — which is not the same as zero, and
    /// the distinction is load-bearing. Nobody can produce this number by
    /// hand: after importing a bank export it is `observed balance today -
    /// SUM(imported transactions)`. So the app never asks for it directly;
    /// it asks for the balance the user can actually read off their bank and
    /// back-solves. Until that happens the anchor is genuinely unknown, and
    /// every balance derived from it is provisional. Collapsing `None` into
    /// `Money::zero` would erase exactly the fact the UI needs in order to
    /// say so.
    opening_balance: Option<Money>,
    description_patterns: Vec<DescriptionPattern>,
}

impl Account {
    /// An account whose starting point is known. Use
    /// [`Self::without_opening_balance`] when it isn't — passing
    /// `Money::from_minor_units(0, …)` here asserts the account genuinely
    /// began at zero.
    pub fn new(id: AccountId, name: AccountName, opening_balance: Money) -> Self {
        Self {
            id,
            name,
            opening_balance: Some(opening_balance),
            description_patterns: Vec::new(),
        }
    }

    /// A newly created account, before any transactions exist to measure a
    /// starting point against. This is what account creation produces — see
    /// the note on [`Self::opening_balance`].
    pub fn without_opening_balance(id: AccountId, name: AccountName) -> Self {
        Self {
            id,
            name,
            opening_balance: None,
            description_patterns: Vec::new(),
        }
    }

    pub fn from_parts(
        id: AccountId,
        name: AccountName,
        opening_balance: Option<Money>,
        description_patterns: Vec<DescriptionPattern>,
    ) -> Self {
        Self {
            id,
            name,
            opening_balance,
            description_patterns,
        }
    }

    pub fn id(&self) -> AccountId {
        self.id
    }

    pub fn name(&self) -> &AccountName {
        &self.name
    }

    pub fn opening_balance(&self) -> Option<&Money> {
        self.opening_balance.as_ref()
    }

    /// The anchor to add to the ledger sum when computing a balance, in
    /// minor units. An unestablished anchor contributes nothing — the
    /// resulting balance is the best the app can do, and
    /// [`Self::is_opening_balance_set`] is how a caller knows to present it
    /// as provisional rather than final.
    pub fn opening_balance_minor_units(&self) -> i64 {
        self.opening_balance
            .as_ref()
            .map(|m| m.minor_units())
            .unwrap_or(0)
    }

    /// Whether the starting point has been established. False means "not
    /// known yet", never "known to be zero".
    pub fn is_opening_balance_set(&self) -> bool {
        self.opening_balance.is_some()
    }

    pub fn description_patterns(&self) -> &[DescriptionPattern] {
        &self.description_patterns
    }

    pub fn rename(&mut self, name: AccountName) {
        self.name = name;
    }

    /// Establishes the starting point. Once set it stays set, including when
    /// set to zero — that's a user assertion the account really did begin at
    /// zero, and re-asking would be treating their answer as no answer.
    pub fn set_opening_balance(&mut self, opening_balance: Money) {
        self.opening_balance = Some(opening_balance);
    }

    pub fn add_description_pattern(&mut self, pattern: DescriptionPattern) {
        if !self.description_patterns.contains(&pattern) {
            self.description_patterns.push(pattern);
        }
    }

    pub fn remove_description_pattern(&mut self, pattern: &DescriptionPattern) {
        self.description_patterns.retain(|p| p != pattern);
    }

    pub fn matches_description(&self, description: &str) -> bool {
        self.description_patterns
            .iter()
            .any(|p| p.matches(description))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, Currency::new("EUR").unwrap())
    }

    #[test]
    fn account_name_rejects_empty_string() {
        assert_eq!(AccountName::new("   "), Err(AccountError::EmptyName));
    }

    #[test]
    fn account_name_trims_whitespace() {
        let name = AccountName::new("  Checking  ").unwrap();
        assert_eq!(name.as_str(), "Checking");
    }

    #[test]
    fn description_pattern_normalizes_case_and_whitespace() {
        let pattern = DescriptionPattern::new("  Whole Foods  ").unwrap();
        assert_eq!(pattern.as_str(), "whole foods");
    }

    #[test]
    fn account_matches_description_when_pattern_is_substring_case_insensitively() {
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            eur(0),
        );
        account.add_description_pattern(DescriptionPattern::new("whole foods").unwrap());

        assert!(account.matches_description("WHOLE FOODS MARKET #123"));
        assert!(!account.matches_description("Trader Joe's"));
    }

    /// The whole point of the `Option`: a brand-new account has an unknown
    /// starting point, and "unknown" must not read as "zero" — the UI keys
    /// off this to warn that balances are provisional.
    #[test]
    fn a_new_account_has_no_opening_balance_but_contributes_zero_to_a_balance() {
        let account = Account::without_opening_balance(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
        );

        assert!(!account.is_opening_balance_set());
        assert_eq!(account.opening_balance(), None);
        assert_eq!(account.opening_balance_minor_units(), 0);
    }

    /// The counterpart case, and the reason a plain `i64` wouldn't do:
    /// someone who genuinely started at zero has answered the question, and
    /// must not be asked again.
    #[test]
    fn an_opening_balance_set_to_zero_still_counts_as_established() {
        let mut account = Account::without_opening_balance(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
        );

        account.set_opening_balance(eur(0));

        assert!(account.is_opening_balance_set());
        assert_eq!(account.opening_balance_minor_units(), 0);
    }

    #[test]
    fn add_description_pattern_does_not_duplicate() {
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            eur(0),
        );
        account.add_description_pattern(DescriptionPattern::new("acme").unwrap());
        account.add_description_pattern(DescriptionPattern::new("ACME").unwrap());

        assert_eq!(account.description_patterns().len(), 1);
    }
}
