//! Money as integer minor units (§32.1).
//!
//! "Native cash calculations use integer minor units or an appropriate decimal
//! type with rule-defined rounding" and "amounts may not be added across
//! currencies without a declared conversion convention". Both rules are
//! enforced here: [`Money`] carries its [`Currency`], the checked operations
//! return [`MoneyError::CurrencyMismatch`], and the plain operators panic with
//! the same message because mixing currencies inside one account or boundary
//! is a programming error, never a user error.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use thiserror::Error;

/// A currency: a three-letter code plus the number of minor-unit digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Currency {
    code: [u8; 3],
    minor_digits: u8,
}

impl Currency {
    /// Builds a currency from its code and minor-unit digits. The code must be
    /// exactly three ASCII characters.
    pub const fn new(code: &str, minor_digits: u8) -> Self {
        let bytes = code.as_bytes();
        assert!(bytes.len() == 3, "currency codes are three ASCII letters");
        Currency {
            code: [bytes[0], bytes[1], bytes[2]],
            minor_digits,
        }
    }

    /// Pakistani rupee, 2 minor digits. The plan's examples are whole numbers
    /// with a `PK-2026-v3` tax-pack label (§19.3), so the fixtures use it.
    pub const PKR: Currency = Currency::new("PKR", 2);
    /// Euro.
    pub const EUR: Currency = Currency::new("EUR", 2);
    /// US dollar.
    pub const USD: Currency = Currency::new("USD", 2);
    /// British pound.
    pub const GBP: Currency = Currency::new("GBP", 2);

    /// The three-letter code.
    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.code).expect("currency codes are ASCII")
    }

    /// Number of digits after the decimal separator.
    pub fn minor_digits(&self) -> u8 {
        self.minor_digits
    }

    /// How many minor units make one major unit (100 for two digits).
    pub fn minor_per_major(&self) -> i64 {
        10_i64.pow(self.minor_digits as u32)
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Why a money operation could not be performed.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MoneyError {
    #[error(
        "currency mismatch: {left} vs {right} (§32.1: amounts may not be added across currencies without a declared conversion)"
    )]
    CurrencyMismatch { left: Currency, right: Currency },
    #[error("arithmetic overflow in minor units")]
    Overflow,
}

/// An exact amount of one currency, stored in minor units.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

impl Money {
    /// An amount from minor units.
    pub const fn new(minor: i64, currency: Currency) -> Self {
        Money { minor, currency }
    }

    /// An amount from whole major units (the plan's examples are all whole).
    pub fn from_major(major: i64, currency: Currency) -> Self {
        let minor = major
            .checked_mul(currency.minor_per_major())
            .expect("major amount fits in i64 minor units");
        Money { minor, currency }
    }

    /// Zero in the given currency.
    pub const fn zero(currency: Currency) -> Self {
        Money { minor: 0, currency }
    }

    /// The amount in minor units.
    pub fn minor(&self) -> i64 {
        self.minor
    }

    /// The currency.
    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn is_zero(&self) -> bool {
        self.minor == 0
    }

    pub fn is_negative(&self) -> bool {
        self.minor < 0
    }

    pub fn is_positive(&self) -> bool {
        self.minor > 0
    }

    fn same_currency(self, other: Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            })
        }
    }

    /// `self + other`, refusing different currencies and overflow.
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// `self - other`, refusing different currencies and overflow.
    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        let minor = self
            .minor
            .checked_sub(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// The same magnitude with the opposite sign.
    pub fn negated(self) -> Money {
        Money::new(-self.minor, self.currency)
    }

    /// The magnitude.
    pub fn abs(self) -> Money {
        Money::new(self.minor.abs(), self.currency)
    }

    /// `max(self, 0)` — §6.6: a displayed spendable amount may be floored at
    /// zero **only** when the deficit is separately reported, so callers must
    /// pair this with [`Money::shortfall_below`].
    pub fn clamped_at_zero(self) -> Money {
        Money::new(self.minor.max(0), self.currency)
    }

    /// `[floor - self]_+` — the shortfall against a required floor (M13).
    pub fn shortfall_below(self, floor: Money) -> Result<Money, MoneyError> {
        Ok(floor.checked_sub(self)?.clamped_at_zero())
    }

    /// The larger of two same-currency amounts.
    pub fn max(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        Ok(if other.minor > self.minor { other } else { self })
    }

    /// The smaller of two same-currency amounts.
    pub fn min(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        Ok(if other.minor < self.minor { other } else { self })
    }

    /// A share of this amount, in basis points (10_000 = 100 %), rounded half
    /// away from zero. Used for ownership attribution (§7) — never for
    /// household aggregation, which counts a joint account once at 100 %.
    pub fn share_basis_points(self, basis_points: u32) -> Money {
        let scaled = self.minor as i128 * basis_points as i128;
        let quotient = scaled / 10_000;
        let remainder = scaled % 10_000;
        let rounded = if remainder.abs() * 2 >= 10_000 {
            quotient + scaled.signum()
        } else {
            quotient
        };
        Money::new(rounded as i64, self.currency)
    }

    /// Sums same-currency amounts; an empty iterator is zero in `currency`.
    pub fn sum<I: IntoIterator<Item = Money>>(
        currency: Currency,
        amounts: I,
    ) -> Result<Money, MoneyError> {
        amounts
            .into_iter()
            .try_fold(Money::zero(currency), Money::checked_add)
    }

    /// Plan-style rendering: thousands separators, minor units only when they
    /// are non-zero (`1,400,000`, `-1,400,000.50`).
    pub fn format(&self) -> String {
        self.render(false)
    }

    /// Always shows the minor digits (`1,400,000.00`).
    pub fn format_exact(&self) -> String {
        self.render(true)
    }

    /// Like [`Money::format`] with an explicit `+` on positive amounts.
    pub fn format_signed(&self) -> String {
        if self.minor > 0 {
            format!("+{}", self.format())
        } else {
            self.format()
        }
    }

    /// `PKR 1,400,000`.
    pub fn format_with_code(&self) -> String {
        format!("{} {}", self.currency, self.format())
    }

    fn render(&self, always_minor: bool) -> String {
        let per_major = self.currency.minor_per_major();
        let magnitude = self.minor.unsigned_abs();
        let major = magnitude / per_major as u64;
        let minor = magnitude % per_major as u64;
        let mut out = String::new();
        if self.minor < 0 {
            out.push('-');
        }
        out.push_str(&group_thousands(major));
        if always_minor || minor != 0 {
            if self.currency.minor_digits > 0 {
                out.push('.');
                out.push_str(&format!(
                    "{:0width$}",
                    minor,
                    width = self.currency.minor_digits as usize
                ));
            }
        }
        out
    }
}

fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// Same-currency invariant: use [`Money::checked_add`] when currencies may differ.
impl Add for Money {
    type Output = Money;
    fn add(self, other: Money) -> Money {
        self.checked_add(other).unwrap_or_else(|err| panic!("{err}"))
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, other: Money) {
        *self = *self + other;
    }
}

/// Same-currency invariant: use [`Money::checked_sub`] when currencies may differ.
impl Sub for Money {
    type Output = Money;
    fn sub(self, other: Money) -> Money {
        self.checked_sub(other).unwrap_or_else(|err| panic!("{err}"))
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, other: Money) {
        *self = *self - other;
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        self.negated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkr(major: i64) -> Money {
        Money::from_major(major, Currency::PKR)
    }

    #[test]
    fn formats_like_the_plan() {
        assert_eq!(pkr(1_400_000).format(), "1,400,000");
        assert_eq!(pkr(650_000).format(), "650,000");
        assert_eq!(pkr(0).format(), "0");
        assert_eq!(pkr(999).format(), "999");
        assert_eq!(pkr(1_000).format(), "1,000");
        assert_eq!(pkr(-20_000).format(), "-20,000");
        assert_eq!(Money::new(140_000_050, Currency::PKR).format(), "1,400,000.50");
        assert_eq!(pkr(5).format_exact(), "5.00");
        assert_eq!(pkr(5).format_signed(), "+5");
        assert_eq!(pkr(1_400_000).format_with_code(), "PKR 1,400,000");
    }

    #[test]
    fn refuses_cross_currency_arithmetic() {
        let err = pkr(1).checked_add(Money::from_major(1, Currency::EUR)).unwrap_err();
        assert!(matches!(err, MoneyError::CurrencyMismatch { .. }));
        assert!(err.to_string().contains("§32.1"));
    }

    #[test]
    #[should_panic(expected = "currency mismatch")]
    fn operators_panic_on_mismatch() {
        let _ = pkr(1) + Money::from_major(1, Currency::EUR);
    }

    #[test]
    fn shortfall_and_clamp_keep_the_deficit_visible() {
        let cash = pkr(980_000);
        let floor = pkr(1_000_000);
        assert_eq!(cash.shortfall_below(floor).unwrap(), pkr(20_000));
        assert_eq!(pkr(1_080_000).shortfall_below(floor).unwrap(), pkr(0));
        assert_eq!(pkr(-30).clamped_at_zero(), pkr(0));
    }

    #[test]
    fn shares_round_half_away_from_zero() {
        assert_eq!(pkr(2_000_000).share_basis_points(5_000), pkr(1_000_000));
        assert_eq!(Money::new(3, Currency::PKR).share_basis_points(5_000), Money::new(2, Currency::PKR));
        assert_eq!(Money::new(-3, Currency::PKR).share_basis_points(5_000), Money::new(-2, Currency::PKR));
        assert_eq!(pkr(100).share_basis_points(10_000), pkr(100));
    }

    #[test]
    fn sums_and_detects_overflow() {
        assert_eq!(Money::sum(Currency::PKR, [pkr(1), pkr(2), pkr(3)]).unwrap(), pkr(6));
        assert_eq!(Money::sum(Currency::PKR, []).unwrap(), pkr(0));
        let huge = Money::new(i64::MAX, Currency::PKR);
        assert_eq!(huge.checked_add(Money::new(1, Currency::PKR)), Err(MoneyError::Overflow));
    }
}
