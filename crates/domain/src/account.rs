use thiserror::Error;
use uuid::Uuid;

use crate::money::Money;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AccountError {
    #[error("account name cannot be empty")]
    EmptyName,
    #[error("account name cannot be longer than {0} characters")]
    NameTooLong(usize),
    #[error("source pattern cannot be empty")]
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
/// transaction's raw source text to auto-assign it to an account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourcePattern(String);

impl SourcePattern {
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

    pub fn matches(&self, source: &str) -> bool {
        source.to_lowercase().contains(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    id: AccountId,
    name: AccountName,
    opening_balance: Money,
    source_patterns: Vec<SourcePattern>,
}

impl Account {
    pub fn new(id: AccountId, name: AccountName, opening_balance: Money) -> Self {
        Self {
            id,
            name,
            opening_balance,
            source_patterns: Vec::new(),
        }
    }

    pub fn from_parts(
        id: AccountId,
        name: AccountName,
        opening_balance: Money,
        source_patterns: Vec<SourcePattern>,
    ) -> Self {
        Self {
            id,
            name,
            opening_balance,
            source_patterns,
        }
    }

    pub fn id(&self) -> AccountId {
        self.id
    }

    pub fn name(&self) -> &AccountName {
        &self.name
    }

    pub fn opening_balance(&self) -> &Money {
        &self.opening_balance
    }

    pub fn source_patterns(&self) -> &[SourcePattern] {
        &self.source_patterns
    }

    pub fn rename(&mut self, name: AccountName) {
        self.name = name;
    }

    pub fn set_opening_balance(&mut self, opening_balance: Money) {
        self.opening_balance = opening_balance;
    }

    pub fn add_source_pattern(&mut self, pattern: SourcePattern) {
        if !self.source_patterns.contains(&pattern) {
            self.source_patterns.push(pattern);
        }
    }

    pub fn remove_source_pattern(&mut self, pattern: &SourcePattern) {
        self.source_patterns.retain(|p| p != pattern);
    }

    pub fn matches_source(&self, source: &str) -> bool {
        self.source_patterns.iter().any(|p| p.matches(source))
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
    fn source_pattern_normalizes_case_and_whitespace() {
        let pattern = SourcePattern::new("  Whole Foods  ").unwrap();
        assert_eq!(pattern.as_str(), "whole foods");
    }

    #[test]
    fn account_matches_source_when_pattern_is_substring_case_insensitively() {
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            eur(0),
        );
        account.add_source_pattern(SourcePattern::new("whole foods").unwrap());

        assert!(account.matches_source("WHOLE FOODS MARKET #123"));
        assert!(!account.matches_source("Trader Joe's"));
    }

    #[test]
    fn add_source_pattern_does_not_duplicate() {
        let mut account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            eur(0),
        );
        account.add_source_pattern(SourcePattern::new("acme").unwrap());
        account.add_source_pattern(SourcePattern::new("ACME").unwrap());

        assert_eq!(account.source_patterns().len(), 1);
    }
}
