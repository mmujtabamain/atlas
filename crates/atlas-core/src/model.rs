//! The household model (§5, §6, §7, §8, §17): people, companies, accounts,
//! ownership, reservations, assumptions, scenarios and tax rules.
//!
//! The model is deliberately record-like: the UI is *verbose* and shows every
//! property in §7's account list, so the fields are public and documented
//! with the plan section they implement. Calculations live in the engine
//! modules ([`crate::liquidity`], [`crate::forecast`]), never here.

use crate::authz::AccessPolicy;
use crate::ids::*;
use crate::money::{Currency, Money};
use crate::timeline::EventSeries;
use crate::vocab::Certainty;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// §5.15 — household-level roles are conveniences for grants, not policy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum HouseholdRole {
    Owner,
    Member,
    Dependent,
    Adviser,
    ReadOnly,
}

impl HouseholdRole {
    pub fn label(self) -> &'static str {
        match self {
            HouseholdRole::Owner => "Owner",
            HouseholdRole::Member => "Household member",
            HouseholdRole::Dependent => "Dependent",
            HouseholdRole::Adviser => "Adviser",
            HouseholdRole::ReadOnly => "Read-only participant",
        }
    }
}

/// §5.2 — a human participant.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub name: String,
    pub role: HouseholdRole,
}

/// §7 — an ownership share in basis points (10_000 = 100 %).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OwnershipShare {
    pub person: PersonId,
    pub basis_points: u32,
}

/// §5.16 — a role within a company; never confers household access (§8.7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EntityRole {
    OwnerDirector,
    FinanceAdministrator,
    PayrollOperator,
    Employee,
    ReadOnlyAdviser,
}

impl EntityRole {
    pub fn label(self) -> &'static str {
        match self {
            EntityRole::OwnerDirector => "Owner / director",
            EntityRole::FinanceAdministrator => "Finance administrator",
            EntityRole::PayrollOperator => "Payroll operator",
            EntityRole::Employee => "Employee",
            EntityRole::ReadOnlyAdviser => "Read-only adviser",
        }
    }
}

/// A person's role inside a company.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EntityRoleAssignment {
    pub person: PersonId,
    pub role: EntityRole,
}

/// §8.6 — company-level constraints the engine treats as constraints, not
/// suggestions.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum CompanyConstraint {
    /// "Do not reduce working capital below X."
    MinimumWorkingCapital(Money),
    /// "Maintain at least N months of payroll."
    PayrollMonthsReserve(u8),
    /// "Maintain tax reserve."
    TaxReserve(Money),
    /// "Do not distribute funds before a specified date."
    NoDistributionBefore(NaiveDate),
    /// "Salary cannot exceed X without explicit override."
    MaximumOwnerSalary(Money),
}

impl CompanyConstraint {
    pub fn describe(&self) -> String {
        match self {
            CompanyConstraint::MinimumWorkingCapital(m) => format!("Keep working capital at or above {}", m.format()),
            CompanyConstraint::PayrollMonthsReserve(n) => format!("Keep at least {n} months of payroll in reserve"),
            CompanyConstraint::TaxReserve(m) => format!("Keep a tax reserve of {}", m.format()),
            CompanyConstraint::NoDistributionBefore(d) => format!("No distributions before {d}"),
            CompanyConstraint::MaximumOwnerSalary(m) => format!("Owner salary at most {} per month without explicit override", m.format()),
        }
    }
}

/// §8.3 — an employee contract, enough for payroll obligations.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Employee {
    pub name: String,
    /// The household person if the employee is one (owner salary, §8.4).
    pub person: Option<PersonId>,
    pub monthly_gross: Money,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
}

/// §5.3 — a separate economic entity.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Company {
    pub id: CompanyId,
    pub name: String,
    /// Free text; no jurisdiction is inferred (§12.7).
    pub jurisdiction: String,
    pub owners: Vec<OwnershipShare>,
    pub roles: Vec<EntityRoleAssignment>,
    pub employees: Vec<Employee>,
    pub constraints: Vec<CompanyConstraint>,
}

/// §5.4 — kinds of account. Sign convention: liabilities carry negative
/// settled balances.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AccountKind {
    Checking,
    Savings,
    CashWallet,
    CreditCard,
    Loan,
    Brokerage,
    FixedDeposit,
    TaxReserve,
    CompanyOperating,
    CompanyPayroll,
    CorporateCard,
}

impl AccountKind {
    pub fn label(self) -> &'static str {
        match self {
            AccountKind::Checking => "Checking",
            AccountKind::Savings => "Savings",
            AccountKind::CashWallet => "Cash wallet",
            AccountKind::CreditCard => "Credit card",
            AccountKind::Loan => "Loan",
            AccountKind::Brokerage => "Brokerage",
            AccountKind::FixedDeposit => "Fixed deposit",
            AccountKind::TaxReserve => "Tax reserve account",
            AccountKind::CompanyOperating => "Company operating",
            AccountKind::CompanyPayroll => "Company payroll",
            AccountKind::CorporateCard => "Corporate card",
        }
    }

    /// Whether balances of this kind are debt (§6.2).
    pub fn is_liability(self) -> bool {
        matches!(self, AccountKind::CreditCard | AccountKind::Loan | AccountKind::CorporateCard)
    }

    /// Whether this kind holds cash that can count as liquid (§6.4) — subject
    /// to the account's own [`Liquidity`] classification.
    pub fn is_cash(self) -> bool {
        matches!(
            self,
            AccountKind::Checking
                | AccountKind::Savings
                | AccountKind::CashWallet
                | AccountKind::TaxReserve
                | AccountKind::CompanyOperating
                | AccountKind::CompanyPayroll
        )
    }
}

/// §7 — liquidity classification.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Liquidity {
    Immediate,
    /// Accessible after a transfer/notice period.
    Delayed { business_days: u16 },
    /// Not accessible before the date (fixed deposit).
    LockedUntil(NaiveDate),
}

impl Liquidity {
    pub fn describe(self) -> String {
        match self {
            Liquidity::Immediate => "Immediate".into(),
            Liquidity::Delayed { business_days } => format!("Delayed {business_days} business days"),
            Liquidity::LockedUntil(date) => format!("Locked until {date}"),
        }
    }
}

/// §7 — where the balance comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SourceOfTruth {
    Manual,
    Imported,
    Synchronized,
}

impl SourceOfTruth {
    pub fn label(self) -> &'static str {
        match self {
            SourceOfTruth::Manual => "Manual",
            SourceOfTruth::Imported => "Imported",
            SourceOfTruth::Synchronized => "Synchronized",
        }
    }
}

/// Who holds an account (§7): people with shares, or a company.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Holder {
    Persons(Vec<OwnershipShare>),
    Company(CompanyId),
}

impl Holder {
    pub fn is_company(&self) -> bool {
        matches!(self, Holder::Company(_))
    }

    /// The share of `person`, in basis points.
    pub fn share_of(&self, person: PersonId) -> u32 {
        match self {
            Holder::Persons(shares) => shares.iter().filter(|s| s.person == person).map(|s| s.basis_points).sum(),
            Holder::Company(_) => 0,
        }
    }
}

/// A fee schedule entry (§7 "Fees").
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Fee {
    pub description: String,
    pub fixed: Option<Money>,
    pub basis_points: u32,
}

/// §5.4 / §7 — an account with every property the plan lists.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub institution: String,
    pub kind: AccountKind,
    pub holder: Holder,
    pub currency: Currency,
    pub liquidity: Liquidity,
    pub minimum_balance: Option<Money>,
    pub transfer_delay_days: u16,
    pub fees: Vec<Fee>,
    pub tax_treatment: String,
    pub source_of_truth: SourceOfTruth,
    pub last_reconciled: Option<NaiveDate>,
    pub withdrawals_permitted: bool,
    /// Categories of expense this account may fund; empty = any.
    pub funds_categories: Vec<String>,
    /// §7 "Household inclusion status".
    pub include_in_household: bool,
    /// §6.4 settled, reconciled ledger cash (negative for liabilities).
    pub settled_balance: Money,
    /// Posted but not yet settled (M01 "pending cash").
    pub pending_balance: Money,
}

impl Account {
    pub fn is_company_account(&self) -> bool {
        self.holder.is_company()
    }
}

/// §17 — how a reservation relates to other floors on the same account.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Coverage {
    /// Independent of every other earmark: it is additive.
    Disjoint,
    /// Already includes the account's bank minimum, so that minimum is not
    /// deducted again (§17, M02).
    CoversAccountMinimum,
    /// Sits inside another reservation and adds nothing on top of it.
    NestedIn(ReservationId),
}

/// §17 — hard constraint or user-relaxable preference.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Hardness {
    Hard,
    SoftUserRelaxable,
}

impl Hardness {
    pub fn label(self) -> &'static str {
        match self {
            Hardness::Hard => "Hard constraint",
            Hardness::SoftUserRelaxable => "User-relaxable preference",
        }
    }
}

/// §5.8 — a claim on existing money without a separate bank account.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Reservation {
    pub id: ReservationId,
    pub name: String,
    pub account: AccountId,
    pub amount: Money,
    pub coverage: Coverage,
    pub hardness: Hardness,
    pub purpose: String,
    /// Set when the obligation was paid and the earmark released (E01).
    pub released_on: Option<NaiveDate>,
}

impl Reservation {
    pub fn is_active(&self) -> bool {
        self.released_on.is_none()
    }
}

/// §2.5 / §10.7 — where an assumption came from.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum AssumptionSource {
    UserEntered,
    DerivedFromHistory {
        formula: String,
        sample_size: u32,
        sample_from: NaiveDate,
        sample_to: NaiveDate,
    },
    RulePack(String),
    Scenario(ScenarioId),
}

impl AssumptionSource {
    pub fn describe(&self) -> String {
        match self {
            AssumptionSource::UserEntered => "Entered by the user".into(),
            AssumptionSource::DerivedFromHistory { formula, sample_size, sample_from, sample_to } => {
                format!("Derived from {sample_size} reconciled payments ({sample_from} – {sample_to}) by: {formula}")
            }
            AssumptionSource::RulePack(pack) => format!("Imported from rule pack {pack}"),
            AssumptionSource::Scenario(id) => format!("Created by scenario {id}"),
        }
    }
}

/// §5.9 — a condition a forecast depends on.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Assumption {
    pub id: AssumptionId,
    pub text: String,
    pub certainty: Certainty,
    pub source: AssumptionSource,
    pub accepted_on: Option<NaiveDate>,
    pub applies_to: Vec<SeriesId>,
    /// Private assumptions belong to a person (§18.5).
    pub private_to: Option<PersonId>,
}

/// §5.11 — an overlay over the baseline (M8 adds the overrides).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub id: ScenarioId,
    pub name: String,
    pub description: String,
    pub private_to: Option<PersonId>,
}

/// §12.1 — how a threshold in a tax rule is measured (§14.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ThresholdBasis {
    PerTransaction,
    PerDay,
    Cumulative,
}

impl ThresholdBasis {
    pub fn label(self) -> &'static str {
        match self {
            ThresholdBasis::PerTransaction => "per transaction",
            ThresholdBasis::PerDay => "per day, all transactions aggregated",
            ThresholdBasis::Cumulative => "cumulative over the period",
        }
    }
}

/// §12.1 — one effective-dated tax rule (M6 evaluates them).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TaxRule {
    pub id: TaxRuleId,
    pub name: String,
    pub tax_type: String,
    pub scope: String,
    pub rate_basis_points: u32,
    pub threshold: Option<Money>,
    pub threshold_basis: ThresholdBasis,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub explanation: String,
}

/// §12.1 / §25 — a versioned pack of tax rules.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TaxRulePack {
    pub name: String,
    pub version: String,
    pub jurisdiction: String,
    /// §12.7: built-in verified rules vs. user-authored / fictitious ones.
    pub verified: bool,
    pub rules: Vec<TaxRule>,
}

/// §5.1 — the planning boundary.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Household {
    pub name: String,
    pub base_currency: Currency,
    /// The date balances were reconciled to; forecasts start here.
    pub as_of: NaiveDate,
    pub people: Vec<Person>,
    pub companies: Vec<Company>,
    pub accounts: Vec<Account>,
    pub reservations: Vec<Reservation>,
    pub series: Vec<EventSeries>,
    pub assumptions: Vec<Assumption>,
    pub scenarios: Vec<Scenario>,
    pub tax_packs: Vec<TaxRulePack>,
    pub policies: Vec<AccessPolicy>,
}

impl Household {
    pub fn person(&self, id: PersonId) -> Option<&Person> {
        self.people.iter().find(|p| p.id == id)
    }

    pub fn company(&self, id: CompanyId) -> Option<&Company> {
        self.companies.iter().find(|c| c.id == id)
    }

    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn reservation(&self, id: ReservationId) -> Option<&Reservation> {
        self.reservations.iter().find(|r| r.id == id)
    }

    pub fn scenario(&self, id: ScenarioId) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.id == id)
    }

    /// Active reservations on one account.
    pub fn active_reservations_on(&self, account: AccountId) -> impl Iterator<Item = &Reservation> {
        self.reservations.iter().filter(move |r| r.account == account && r.is_active())
    }

    /// The name of an entity, for labels.
    pub fn entity_name(&self, entity: EntityRef) -> String {
        match entity {
            EntityRef::Household => self.name.clone(),
            EntityRef::Person(id) => self.person(id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_string()),
            EntityRef::Company(id) => self.company(id).map(|c| c.name.clone()).unwrap_or_else(|| id.to_string()),
        }
    }

    /// A readable description of who holds an account, with shares.
    pub fn holder_description(&self, account: &Account) -> String {
        match &account.holder {
            Holder::Company(id) => self.entity_name(EntityRef::Company(*id)),
            Holder::Persons(shares) => shares
                .iter()
                .map(|s| {
                    let name = self.entity_name(EntityRef::Person(s.person));
                    if s.basis_points == 10_000 {
                        name
                    } else {
                        format!("{name} {}%", s.basis_points / 100)
                    }
                })
                .collect::<Vec<_>>()
                .join(" · "),
        }
    }

    /// Accounts held by a company.
    pub fn company_accounts(&self, company: CompanyId) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(move |a| a.holder == Holder::Company(company))
    }

    /// Accounts a person holds, in whole or part.
    pub fn accounts_of(&self, person: PersonId) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(move |a| a.holder.share_of(person) > 0)
    }

    /// Companies a person owns, in whole or part.
    pub fn companies_of(&self, person: PersonId) -> impl Iterator<Item = &Company> {
        self.companies.iter().filter(move |c| c.owners.iter().any(|o| o.person == person))
    }
}
