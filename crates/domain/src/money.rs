use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("currency code must be 3 uppercase ASCII letters, got {0:?}")]
    InvalidCurrencyCode(String),
    #[error("cannot combine amounts in different currencies ({0} vs {1})")]
    CurrencyMismatch(String, String),
}

/// ISO 4217-shaped currency code (e.g. "USD", "EUR"). Not validated against
/// the real ISO 4217 list — just the 3-uppercase-letter shape — since the
/// app only needs internal consistency, not exchange-rate correctness.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Currency(String);

impl Currency {
    pub fn new(code: &str) -> Result<Self, MoneyError> {
        let code = code.trim();
        let is_valid = code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase());
        if !is_valid {
            return Err(MoneyError::InvalidCurrencyCode(code.to_string()));
        }
        Ok(Self(code.to_string()))
    }

    pub fn code(&self) -> &str {
        &self.0
    }
}

/// An amount of money in integer minor units (e.g. cents) — never `f64`, to
/// avoid floating-point rounding errors on financial totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    minor_units: i64,
    currency: Currency,
}

impl Money {
    pub fn from_minor_units(minor_units: i64, currency: Currency) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    pub fn zero(currency: Currency) -> Self {
        Self::from_minor_units(0, currency)
    }

    pub fn minor_units(&self) -> i64 {
        self.minor_units
    }

    pub fn currency(&self) -> &Currency {
        &self.currency
    }

    pub fn is_negative(&self) -> bool {
        self.minor_units < 0
    }

    pub fn add(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(
                self.currency.code().to_string(),
                other.currency.code().to_string(),
            ));
        }
        Ok(Money::from_minor_units(
            self.minor_units + other.minor_units,
            self.currency.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_accepts_three_uppercase_letters() {
        assert!(Currency::new("USD").is_ok());
    }

    #[test]
    fn currency_rejects_lowercase_or_wrong_length() {
        assert!(Currency::new("usd").is_err());
        assert!(Currency::new("US").is_err());
        assert!(Currency::new("DOLLARS").is_err());
    }

    #[test]
    fn money_add_combines_minor_units_in_same_currency() {
        let usd = Currency::new("USD").unwrap();
        let a = Money::from_minor_units(1000, usd.clone());
        let b = Money::from_minor_units(250, usd);

        let sum = a.add(&b).unwrap();

        assert_eq!(sum.minor_units(), 1250);
    }

    #[test]
    fn money_add_rejects_mismatched_currency() {
        let a = Money::from_minor_units(100, Currency::new("USD").unwrap());
        let b = Money::from_minor_units(100, Currency::new("EUR").unwrap());

        let result = a.add(&b);

        assert!(matches!(result, Err(MoneyError::CurrencyMismatch(_, _))));
    }
}
