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
use chrono::{Datelike, NaiveDate};
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

    /// The entity a posting on the account is attributed to by default: the
    /// company, the sole holder, or the household for a joint account.
    pub fn primary_entity(&self) -> EntityRef {
        match self {
            Holder::Company(id) => EntityRef::Company(*id),
            Holder::Persons(shares) if shares.len() == 1 => EntityRef::Person(shares[0].person),
            Holder::Persons(_) => EntityRef::Household,
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
    /// F117 — after this date the assumption must be re-approved.
    pub expires_on: Option<NaiveDate>,
    pub applies_to: Vec<SeriesId>,
    /// Private assumptions belong to a person (§18.5).
    pub private_to: Option<PersonId>,
}

/// F117 — how current an assumption is on a given date.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Freshness {
    NotAccepted,
    Fresh,
    /// Accepted more than `STALE_AFTER_DAYS` ago and never re-approved.
    Stale,
    Expired,
}

impl Freshness {
    pub fn label(self) -> &'static str {
        match self {
            Freshness::NotAccepted => "not accepted",
            Freshness::Fresh => "fresh",
            Freshness::Stale => "stale",
            Freshness::Expired => "expired",
        }
    }
}

/// Assumptions accepted longer ago than this are flagged stale (F117).
pub const STALE_AFTER_DAYS: i64 = 90;

impl Assumption {
    pub fn freshness(&self, on: NaiveDate) -> Freshness {
        if let Some(expiry) = self.expires_on
            && on > expiry
        {
            return Freshness::Expired;
        }
        match self.accepted_on {
            None => Freshness::NotAccepted,
            Some(accepted) if (on - accepted).num_days() > STALE_AFTER_DAYS => Freshness::Stale,
            Some(_) => Freshness::Fresh,
        }
    }
}

/// §10.7 — one reconciled historical payment of a series, the raw material of
/// a derived assumption.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct HistoricalPayment {
    pub series: SeriesId,
    pub date: NaiveDate,
    pub amount: Money,
}

/// §5.11 — an overlay over the baseline (M8 adds the overrides).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub id: ScenarioId,
    pub name: String,
    pub description: String,
    pub private_to: Option<PersonId>,
    /// §18.2 — explicit changes of the overlay.
    #[serde(default)]
    pub changes: Vec<crate::scenario::ScenarioChange>,
    /// §18.3 — scenarios this one is composed of.
    #[serde(default)]
    pub composed_of: Vec<ScenarioId>,
}

impl Scenario {
    /// Whether the scenario changes anything beyond the objects tagged with it.
    pub fn has_overlay(&self) -> bool {
        !self.changes.is_empty() || !self.composed_of.is_empty()
    }
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

/// M23 — one marginal bracket: `rate` applies to the part of the base between
/// `lower` and `upper` (`None` = unbounded top bracket).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Bracket {
    pub lower: Money,
    pub upper: Option<Money>,
    pub rate_basis_points: u32,
}

/// §12.3 — when the tax cash actually moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TaxTiming {
    /// Paid on the transaction date.
    Immediate,
    /// Withheld at source on the transaction date; `creditable` at assessment.
    WithheldAtSource { creditable: bool },
    /// Incurred over a calendar year, payable on `month`/`day` of the next year.
    AnnualAssessment { due_month: u8, due_day: u8 },
}

impl TaxTiming {
    pub fn label(self) -> String {
        match self {
            TaxTiming::Immediate => "paid immediately".into(),
            TaxTiming::WithheldAtSource { creditable: true } => "withheld at source, creditable at assessment".into(),
            TaxTiming::WithheldAtSource { creditable: false } => "withheld at source, final (not creditable)".into(),
            TaxTiming::AnnualAssessment { due_month, due_day } => format!("accrued over the calendar year, payable {due_day}/{due_month} of the next year"),
        }
    }
}

/// §12.1 — what a rule computes.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum TaxKind {
    /// A flat rate on the full transaction amount once it exceeds a threshold
    /// measured on `basis` (§14.3: full amount vs excess must be explicit).
    FlatAboveThreshold { rate_basis_points: u32, threshold: Money, basis: ThresholdBasis, on_excess_only: bool },
    /// A flat percentage of every matching transaction.
    FlatRate { rate_basis_points: u32 },
    /// Marginal brackets on the entity's annual taxable base (M23).
    AnnualBrackets { brackets: Vec<Bracket> },
}

/// §12.1 — one effective-dated tax rule.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TaxRule {
    pub id: TaxRuleId,
    pub name: String,
    pub tax_type: String,
    /// Series categories the rule applies to (e.g. "Salary", "Cash withdrawal").
    pub categories: Vec<String>,
    pub scope: String,
    pub kind: TaxKind,
    pub timing: TaxTiming,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    /// Official source / reference metadata, or the fictitious-example note.
    pub source: String,
    pub explanation: String,
}

impl TaxRule {
    pub fn is_effective_on(&self, date: NaiveDate) -> bool {
        date >= self.effective_from && self.effective_to.is_none_or(|end| date <= end)
    }

    pub fn applies_to_category(&self, category: &str) -> bool {
        self.categories.iter().any(|c| c.eq_ignore_ascii_case(category))
    }

    pub fn describe_kind(&self) -> String {
        match &self.kind {
            TaxKind::FlatAboveThreshold { rate_basis_points, threshold, basis, on_excess_only } => format!(
                "{}.{:02}% on the {} once a transaction exceeds {} ({})",
                rate_basis_points / 100,
                rate_basis_points % 100,
                if *on_excess_only { "excess" } else { "full amount" },
                threshold.format(),
                basis.label()
            ),
            TaxKind::FlatRate { rate_basis_points } => format!("{}.{:02}% of every matching transaction", rate_basis_points / 100, rate_basis_points % 100),
            TaxKind::AnnualBrackets { brackets } => brackets
                .iter()
                .map(|b| match b.upper {
                    Some(upper) => format!("{}% from {} to {}", b.rate_basis_points / 100, b.lower.format(), upper.format()),
                    None => format!("{}% above {}", b.rate_basis_points / 100, b.lower.format()),
                })
                .collect::<Vec<_>>()
                .join(" · "),
        }
    }
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

/// §5.7 — a movement that actually happened, reconcilable to a planned occurrence.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActualTransaction {
    pub id: TransactionId,
    pub date: NaiveDate,
    pub account: AccountId,
    /// Signed: positive money in, negative money out.
    pub amount: Money,
    pub description: String,
}

/// §16 — links an actual transaction to the planned occurrence it fulfils
/// (in full or in part).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReconciliationLink {
    pub series: SeriesId,
    /// The occurrence's original due date (its identity within the series).
    pub original_due: NaiveDate,
    pub transaction: TransactionId,
    /// How much of the planned amount this transaction fulfils.
    pub amount: Money,
}

/// Bumped whenever a saved household would no longer load as-is.
pub const SCHEMA_VERSION: u32 = 1;

/// §20 — a goal: an amount the household wants available (above its reserve)
/// by a target date. Deterministic scheduling, not advice.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub name: String,
    pub amount: Money,
    pub target_on: NaiveDate,
    /// Lower is more important (§20 competing uses of money).
    pub priority: u8,
    pub private_to: Option<PersonId>,
}

/// §5.1 — the planning boundary.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
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
    pub actuals: Vec<ActualTransaction>,
    pub links: Vec<ReconciliationLink>,
    /// §10.7 — reconciled history behind derived assumptions.
    pub history: Vec<HistoricalPayment>,
    /// §14 — user-defined rules.
    pub rules: Vec<crate::rules::Rule>,
    /// §14.7 — how ties between equally specific, equal-priority rules are broken.
    pub rule_tie_break: crate::rules::TieBreak,
    /// §20 — goals competing for the same money.
    pub goals: Vec<Goal>,
    /// §5.14 / §7.4 — purpose-specific grants.
    pub grants: Vec<crate::authz::AccessGrant>,
    /// §5.18 — the immutable privacy audit log.
    pub audit: Vec<crate::authz::PrivacyAuditEvent>,
}

impl Default for Household {
    fn default() -> Self {
        Household::empty("New household", Currency::USD, NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid"))
    }
}

impl Household {
    /// An empty household: no people, no accounts, nothing assumed (§2.5).
    pub fn empty(name: &str, base_currency: Currency, as_of: NaiveDate) -> Self {
        Household {
            name: name.into(),
            base_currency,
            as_of,
            people: Vec::new(),
            companies: Vec::new(),
            accounts: Vec::new(),
            reservations: Vec::new(),
            series: Vec::new(),
            assumptions: Vec::new(),
            scenarios: Vec::new(),
            tax_packs: Vec::new(),
            policies: Vec::new(),
            actuals: Vec::new(),
            links: Vec::new(),
            history: Vec::new(),
            rules: Vec::new(),
            rule_tie_break: crate::rules::TieBreak::OldestRule,
            goals: Vec::new(),
            grants: Vec::new(),
            audit: Vec::new(),
        }
    }

    /// The whole household as JSON (the persistence layer stores this shape).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Household> {
        serde_json::from_str(json)
    }

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
            EntityRef::Person(id) => self.person(id).map(|p| p.name.clone()).unwrap_or_else(|| if self.people.is_empty() { "no one yet".to_string() } else { id.to_string() }),
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

    pub fn series_by_id(&self, id: SeriesId) -> Option<&EventSeries> {
        self.series.iter().find(|s| s.id == id)
    }

    pub fn actual(&self, id: TransactionId) -> Option<&ActualTransaction> {
        self.actuals.iter().find(|t| t.id == id)
    }

    pub fn assumption(&self, id: AssumptionId) -> Option<&Assumption> {
        self.assumptions.iter().find(|a| a.id == id)
    }

    /// Reconciled history of one series, oldest first.
    pub fn history_of(&self, series: SeriesId) -> Vec<&HistoricalPayment> {
        let mut rows: Vec<&HistoricalPayment> = self.history.iter().filter(|h| h.series == series).collect();
        rows.sort_by_key(|h| h.date);
        rows
    }

    /// §2.5 — records the user's acceptance of an assumption on `on`.
    pub fn accept_assumption(&mut self, id: AssumptionId, on: NaiveDate) -> crate::EngineResult<()> {
        let assumption = self.assumptions.iter_mut().find(|a| a.id == id).ok_or(crate::EngineError::UnknownAssumption(id))?;
        assumption.accepted_on = Some(on);
        Ok(())
    }

    /// Reconciliation links for one occurrence.
    pub fn links_for(&self, series: SeriesId, original_due: NaiveDate) -> impl Iterator<Item = &ReconciliationLink> {
        self.links.iter().filter(move |l| l.series == series && l.original_due == original_due)
    }

    /// §16 — expands a series and applies reconciliation and the calendar:
    /// fulfilled / partially fulfilled from links, due / overdue relative to
    /// `as_of`. Occurrences due on or before `after` are excluded, except
    /// those that are still open (overdue) when `after == as_of`.
    pub fn expand_series(&self, series: &EventSeries, after: NaiveDate, through: NaiveDate) -> Vec<crate::timeline::Occurrence> {
        use crate::timeline::OccurrenceStatus;
        let mut occurrences = crate::timeline::expand(series, after, through);
        for occurrence in &mut occurrences {
            let fulfilled = self
                .links_for(series.id, occurrence.original_due)
                .try_fold(Money::zero(occurrence.amount.currency()), |acc, link| acc.checked_add(link.amount))
                .unwrap_or(Money::zero(occurrence.amount.currency()));
            occurrence.fulfilled = fulfilled;
            if matches!(occurrence.status, OccurrenceStatus::Skipped | OccurrenceStatus::Cancelled) {
                continue;
            }
            occurrence.status = if fulfilled.minor() >= occurrence.amount.expected().minor() && fulfilled.is_positive() {
                OccurrenceStatus::Fulfilled
            } else if fulfilled.is_positive() {
                OccurrenceStatus::PartiallyFulfilled
            } else if occurrence.due < self.as_of {
                OccurrenceStatus::Overdue
            } else if occurrence.due == self.as_of {
                OccurrenceStatus::Due
            } else {
                OccurrenceStatus::Planned
            };
        }
        occurrences
    }

    /// Every occurrence of every series in the window, chronologically, with
    /// the given scenario overlay (§18) applied.
    pub fn expand_all(&self, after: NaiveDate, through: NaiveDate, scenario: Option<ScenarioId>) -> Vec<crate::timeline::Occurrence> {
        if let Some(id) = scenario
            && self.scenario(id).is_some_and(|s| s.has_overlay())
        {
            match self.apply_scenarios(&[id]) {
                Ok(overlaid) => return overlaid.expand_all(after, through, scenario),
                Err(err) => log::warn!("scenario {id} overlay not applied: {err}"),
            }
        }
        let mut all: Vec<_> = self
            .series
            .iter()
            .filter(|s| s.scenario.is_none() || s.scenario == scenario)
            .flat_map(|s| self.expand_series(s, after, through))
            .collect();
        all.sort_by_key(|o| o.sort_key());
        all
    }

    /// The next free tax rule id across every pack.
    pub fn next_tax_rule_id(&self) -> TaxRuleId {
        TaxRuleId::new(self.tax_packs.iter().flat_map(|p| p.rules.iter()).map(|r| r.id.raw()).max().unwrap_or(0) + 1)
    }

    /// Every series category in use, sorted (for rule scopes).
    pub fn categories(&self) -> Vec<String> {
        let mut categories: Vec<String> = self.series.iter().map(|s| s.category.clone()).collect();
        categories.sort();
        categories.dedup();
        categories
    }

    /// §12.2 — adds a user-authored rule to the unverified user pack of its
    /// effective year, creating the pack if needed. User rules never claim
    /// verification (§12.7).
    pub fn add_user_tax_rule(&mut self, rule: TaxRule) -> String {
        let year = rule.effective_from.year();
        let name = format!("USER-DEFINED-{year}-v1");
        match self.tax_packs.iter_mut().find(|p| p.name == name) {
            Some(pack) => pack.rules.push(rule),
            None => self.tax_packs.push(TaxRulePack {
                name: name.clone(),
                version: "v1".into(),
                jurisdiction: "User-authored — unverified until an official source is attached (§12.7)".into(),
                verified: false,
                rules: vec![rule],
            }),
        }
        name
    }

    /// The next free reservation id.
    pub fn next_reservation_id(&self) -> ReservationId {
        ReservationId::new(self.reservations.iter().map(|r| r.id.raw()).max().unwrap_or(0) + 1)
    }

    /// Adds an earmark (§17). The account must exist and share the currency.
    pub fn add_reservation(&mut self, reservation: Reservation) -> crate::EngineResult<ReservationId> {
        let account = self.account(reservation.account).ok_or(crate::EngineError::UnknownAccount(reservation.account))?;
        if account.currency != reservation.amount.currency() {
            return Err(crate::MoneyError::CurrencyMismatch { left: account.currency, right: reservation.amount.currency() }.into());
        }
        if let Coverage::NestedIn(outer) = reservation.coverage {
            self.reservation(outer).ok_or(crate::EngineError::UnknownReservation(outer))?;
        }
        let id = reservation.id;
        self.reservations.push(reservation);
        Ok(id)
    }

    /// E01: pays the obligation an earmark was held for and releases the
    /// earmark — cash falls by the amount, the reservation stops constraining,
    /// and free cash is unchanged. Returns the new settled balance.
    pub fn pay_and_release(&mut self, id: ReservationId, on: NaiveDate) -> crate::EngineResult<Money> {
        let (account_id, amount) = {
            let reservation = self.reservation(id).ok_or(crate::EngineError::UnknownReservation(id))?;
            (reservation.account, reservation.amount)
        };
        let account = self.accounts.iter_mut().find(|a| a.id == account_id).ok_or(crate::EngineError::UnknownAccount(account_id))?;
        account.settled_balance = account.settled_balance.checked_sub(amount)?;
        let balance = account.settled_balance;
        let reservation = self.reservations.iter_mut().find(|r| r.id == id).expect("checked above");
        reservation.released_on = Some(on);
        Ok(balance)
    }
}

#[cfg(test)]
mod reconciliation_tests {
    use crate::fixtures::{self, ids, pkr};
    use crate::timeline::OccurrenceStatus;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn statuses_follow_links_and_the_calendar() {
        let household = fixtures::plan_household();
        let insurance = household.series_by_id(ids::CAR_INSURANCE).unwrap();
        // Expanding from before the premium shows it fulfilled by its actual (V012).
        let occurrences = household.expand_series(insurance, d(2026, 9, 1), d(2027, 9, 30));
        assert_eq!(occurrences[0].status, OccurrenceStatus::Fulfilled);
        assert_eq!(occurrences[0].fulfilled, pkr(96_000));
        assert_eq!(occurrences[0].remaining_expected(), pkr(0));
        assert!(!occurrences[0].is_live());
        assert_eq!(occurrences[1].status, OccurrenceStatus::Planned, "next year's premium is planned");

        let receivable = household.series_by_id(ids::CLIENT_RECEIVABLE).unwrap();
        let occurrences = household.expand_series(receivable, household.as_of, d(2027, 1, 31));
        assert_eq!(occurrences[0].status, OccurrenceStatus::PartiallyFulfilled);
        assert_eq!(occurrences[0].remaining_expected(), pkr(200_000));
        assert_eq!(occurrences[0].settlement, d(2026, 11, 12), "two-day settlement lag (M05)");
    }

    #[test]
    fn unpaid_past_occurrences_are_overdue_and_today_is_due() {
        let mut household = fixtures::plan_household();
        household.links.clear();
        let insurance = household.series_by_id(ids::CAR_INSURANCE).unwrap().clone();
        let occurrences = household.expand_series(&insurance, d(2026, 9, 1), d(2026, 12, 31));
        assert_eq!(occurrences[0].status, OccurrenceStatus::Overdue);
        household.as_of = d(2026, 9, 5);
        let occurrences = household.expand_series(&insurance, d(2026, 9, 1), d(2026, 12, 31));
        assert_eq!(occurrences[0].status, OccurrenceStatus::Due);
    }

    #[test]
    fn expand_all_is_chronological_and_respects_scenarios() {
        let household = fixtures::plan_household();
        let all = household.expand_all(household.as_of, d(2027, 1, 31), None);
        assert!(all.windows(2).all(|w| w[0].sort_key() <= w[1].sort_key()));
        assert!(all.iter().all(|o| o.scenario.is_none()));
        let with_car = household.expand_all(household.as_of, d(2027, 1, 31), Some(ids::BUY_CAR));
        assert_eq!(with_car.len(), all.len() + 1);
    }
}
