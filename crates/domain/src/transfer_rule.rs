use thiserror::Error;
use uuid::Uuid;

use crate::account::{AccountId, DescriptionPattern};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransferRuleError {
    #[error("invalid id: {0}")]
    InvalidId(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferRuleId(Uuid);

impl TransferRuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, TransferRuleError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| TransferRuleError::InvalidId(raw.to_string()))
    }

    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TransferRuleId {
    fn default() -> Self {
        Self::new()
    }
}

/// Recognizes an imported row as money moving to another of the user's own
/// accounts, rather than as spending.
///
/// This is deliberately *not* an `Account` description pattern, despite reusing
/// [`DescriptionPattern`] for the matching itself. An account's own patterns
/// answer "which account does this row belong to"; a transfer rule answers
/// "this row belongs to the account being imported, and its counterpart is
/// over there" — a different question about a row whose account is already
/// settled. Overloading one field with both meanings would make a pattern's
/// effect depend on context that isn't visible where it's configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRule {
    id: TransferRuleId,
    pattern: DescriptionPattern,
    counterpart_account_id: AccountId,
}

impl TransferRule {
    pub fn new(
        id: TransferRuleId,
        pattern: DescriptionPattern,
        counterpart_account_id: AccountId,
    ) -> Self {
        Self {
            id,
            pattern,
            counterpart_account_id,
        }
    }

    pub fn id(&self) -> TransferRuleId {
        self.id
    }

    pub fn pattern(&self) -> &DescriptionPattern {
        &self.pattern
    }

    pub fn counterpart_account_id(&self) -> AccountId {
        self.counterpart_account_id
    }

    pub fn matches_description(&self, description: &str) -> bool {
        self.pattern.matches(description)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str) -> TransferRule {
        TransferRule::new(
            TransferRuleId::new(),
            DescriptionPattern::new(pattern).unwrap(),
            AccountId::new(),
        )
    }

    #[test]
    fn matches_description_is_case_insensitive_substring() {
        let rule = rule("n26");
        assert!(rule.matches_description("VIREMENT SEPA N26 BANK"));
        assert!(rule.matches_description("virement n26"));
    }

    #[test]
    fn does_not_match_unrelated_description() {
        let rule = rule("n26");
        assert!(!rule.matches_description("CARTE 12/03 BOULANGERIE"));
    }

    /// The pattern normalizes on the way in, so a rule configured with
    /// stray case or padding still matches — the user typing " N26 " in a
    /// text field shouldn't quietly produce a rule that never fires.
    #[test]
    fn pattern_is_normalized_before_matching() {
        let rule = rule("  N26  ");
        assert_eq!(rule.pattern().as_str(), "n26");
        assert!(rule.matches_description("virement n26"));
    }
}
