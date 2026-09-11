//! Data-entry operations on a household (M12.4, M12.5). Every creation path
//! attaches an [`AccessPolicy`] (F162: an object without a policy would be
//! hidden from everyone, its creator included); ownership shares must total
//! 100 %; currencies must match the base currency; deletion is refused while
//! anything still references the object.

use crate::authz::{AccessPolicy, CalculationAccess, VisibilityPreset};
use crate::ids::*;
use crate::model::*;
use crate::money::Money;
use crate::timeline::{Direction, EventSeries};
use crate::{EngineError, EngineResult};
use chrono::NaiveDate;

impl Household {
    fn next_policy_id(&self) -> PolicyId {
        PolicyId::new(self.policies.iter().map(|p| p.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_person_id(&self) -> PersonId {
        PersonId::new(self.people.iter().map(|p| p.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_company_id(&self) -> CompanyId {
        CompanyId::new(self.companies.iter().map(|c| c.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_account_id(&self) -> AccountId {
        AccountId::new(self.accounts.iter().map(|a| a.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_series_id(&self) -> SeriesId {
        SeriesId::new(self.series.iter().map(|s| s.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_assumption_id(&self) -> AssumptionId {
        AssumptionId::new(self.assumptions.iter().map(|a| a.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_scenario_id(&self) -> ScenarioId {
        ScenarioId::new(self.scenarios.iter().map(|s| s.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_transaction_id(&self) -> TransactionId {
        TransactionId::new(self.actuals.iter().map(|t| t.id.raw()).max().unwrap_or(0) + 1)
    }

    /// Attaches a policy to a new object (fail-closed: no policy, no object).
    fn attach_policy(&mut self, object: ObjectRef, owners: Vec<PersonId>, preset: VisibilityPreset, access: CalculationAccess) {
        let owners = if owners.is_empty() { self.people.first().map(|p| vec![p.id]).unwrap_or_default() } else { owners };
        if owners.is_empty() {
            return;
        }
        let policy = AccessPolicy::preset(self.next_policy_id(), object, owners, preset, access, self.as_of, self.as_of.and_hms_opt(0, 0, 0).expect("midnight"));
        self.policies.push(policy);
    }

    /// Adds a person; they own their own record (fully shared by default).
    pub fn add_person(&mut self, person: Person) -> PersonId {
        let id = person.id;
        self.people.push(person);
        self.attach_policy(ObjectRef::Person(id), vec![id], VisibilityPreset::FullyShared, CalculationAccess::Full);
        id
    }

    /// Adds a company whose owners must hold 100 % between them; household
    /// members see only the planning-safe summary by default (§8.7).
    pub fn add_company(&mut self, company: Company) -> EngineResult<CompanyId> {
        validate_shares(company.owners.iter().map(|o| o.basis_points))?;
        for owner in &company.owners {
            self.person(owner.person).ok_or(EngineError::UnknownPerson(owner.person))?;
        }
        let id = company.id;
        let owners: Vec<PersonId> = company.owners.iter().map(|o| o.person).collect();
        self.companies.push(company);
        self.attach_policy(ObjectRef::Company(id), owners, VisibilityPreset::SharedSummary, CalculationAccess::Excluded);
        if let Some(policy) = self.policies.iter_mut().find(|p| p.object == ObjectRef::Company(id)) {
            policy.existence = crate::authz::Grantees::Household;
        }
        Ok(id)
    }

    /// Adds an account with its policy. Personal accounts default to fully
    /// shared and fully available; company accounts to summary-only and
    /// excluded from household calculations (§8.5).
    pub fn add_account(&mut self, account: Account, preset: VisibilityPreset, access: CalculationAccess) -> EngineResult<AccountId> {
        if account.currency != self.base_currency {
            return Err(crate::MoneyError::CurrencyMismatch { left: self.base_currency, right: account.currency }.into());
        }
        let owners = match &account.holder {
            Holder::Persons(shares) => {
                validate_shares(shares.iter().map(|s| s.basis_points))?;
                for share in shares {
                    self.person(share.person).ok_or(EngineError::UnknownPerson(share.person))?;
                }
                shares.iter().map(|s| s.person).collect()
            }
            Holder::Company(company) => {
                let company = self.company(*company).ok_or(EngineError::UnknownCompany(*company))?;
                company.owners.iter().map(|o| o.person).collect()
            }
        };
        let id = account.id;
        let is_company = account.is_company_account();
        self.accounts.push(account);
        let (preset, access) = if is_company { (VisibilityPreset::SharedSummary, CalculationAccess::Excluded) } else { (preset, access) };
        self.attach_policy(ObjectRef::Account(id), owners, preset, access);
        Ok(id)
    }

    /// Adds a series; it inherits its account's policy (§7.1).
    pub fn add_series(&mut self, series: EventSeries) -> EngineResult<SeriesId> {
        self.account(series.account).ok_or(EngineError::UnknownAccount(series.account))?;
        if series.amount.currency() != self.base_currency {
            return Err(crate::MoneyError::CurrencyMismatch { left: self.base_currency, right: series.amount.currency() }.into());
        }
        if let Direction::Transfer { to } = series.direction {
            self.account(to).ok_or(EngineError::UnknownAccount(to))?;
        }
        if let Some(linked) = series.linked_account {
            self.account(linked).ok_or(EngineError::UnknownAccount(linked))?;
        }
        let id = series.id;
        self.series.push(series);
        Ok(id)
    }

    pub fn add_assumption(&mut self, assumption: Assumption) -> AssumptionId {
        let id = assumption.id;
        self.assumptions.push(assumption);
        id
    }

    /// Adds a scenario; private scenarios get a private policy (§18.5).
    pub fn add_scenario(&mut self, scenario: Scenario, owner: PersonId) -> ScenarioId {
        let id = scenario.id;
        let private = scenario.private_to.is_some();
        self.scenarios.push(scenario);
        let owners = if private { vec![owner] } else { self.people.iter().map(|p| p.id).collect() };
        self.attach_policy(
            ObjectRef::Scenario(id),
            owners,
            if private { VisibilityPreset::Private } else { VisibilityPreset::FullyShared },
            if private { CalculationAccess::Excluded } else { CalculationAccess::Full },
        );
        id
    }

    /// §5.7 — records an actual transaction on an account.
    pub fn add_actual(&mut self, actual: ActualTransaction) -> EngineResult<TransactionId> {
        self.account(actual.account).ok_or(EngineError::UnknownAccount(actual.account))?;
        let id = actual.id;
        self.actuals.push(actual);
        Ok(id)
    }

    /// §16 — links an actual to the planned occurrence it fulfils.
    pub fn link_actual(&mut self, link: ReconciliationLink) -> EngineResult<()> {
        self.series_by_id(link.series).ok_or(EngineError::UnknownSeries(link.series))?;
        self.actual(link.transaction).ok_or_else(|| EngineError::Insufficient(format!("unknown transaction {}", link.transaction)))?;
        self.links.push(link);
        Ok(())
    }

    /// Sets an account's settled balance as of a date; the household's `as_of`
    /// becomes the latest reconciliation (§7 "last reconciliation date").
    pub fn reconcile_account(&mut self, id: AccountId, settled: Money, on: NaiveDate) -> EngineResult<()> {
        let base = self.base_currency;
        let account = self.accounts.iter_mut().find(|a| a.id == id).ok_or(EngineError::UnknownAccount(id))?;
        if settled.currency() != base {
            return Err(crate::MoneyError::CurrencyMismatch { left: base, right: settled.currency() }.into());
        }
        account.settled_balance = settled;
        account.last_reconciled = Some(on);
        if on > self.as_of {
            self.as_of = on;
        }
        Ok(())
    }

    /// Removes an account unless something still references it.
    pub fn remove_account(&mut self, id: AccountId) -> EngineResult<()> {
        let referenced: Vec<String> = self
            .series
            .iter()
            .filter(|s| s.account == id || s.linked_account == Some(id) || matches!(s.direction, Direction::Transfer { to } if to == id))
            .map(|s| format!("series “{}”", s.name))
            .chain(self.reservations.iter().filter(|r| r.account == id).map(|r| format!("reservation “{}”", r.name)))
            .chain(self.actuals.iter().filter(|t| t.account == id).map(|t| format!("actual “{}”", t.description)))
            .collect();
        if !referenced.is_empty() {
            return Err(EngineError::Insufficient(format!("still referenced by {}", referenced.join(", "))));
        }
        self.accounts.retain(|a| a.id != id);
        self.policies.retain(|p| p.object != ObjectRef::Account(id));
        Ok(())
    }

    /// Removes a series and its exceptions; reconciliation links to it go too.
    pub fn remove_series(&mut self, id: SeriesId) -> EngineResult<()> {
        self.series_by_id(id).ok_or(EngineError::UnknownSeries(id))?;
        self.series.retain(|s| s.id != id);
        self.links.retain(|l| l.series != id);
        for assumption in &mut self.assumptions {
            assumption.applies_to.retain(|s| *s != id);
        }
        Ok(())
    }

    pub fn remove_reservation(&mut self, id: ReservationId) -> EngineResult<()> {
        if self.reservations.iter().any(|r| matches!(r.coverage, Coverage::NestedIn(outer) if outer == id)) {
            return Err(EngineError::Insufficient("another earmark nests inside it".into()));
        }
        self.reservation(id).ok_or(EngineError::UnknownReservation(id))?;
        self.reservations.retain(|r| r.id != id);
        Ok(())
    }

    /// Removes a person unless they hold accounts, own companies or have series.
    pub fn remove_person(&mut self, id: PersonId) -> EngineResult<()> {
        let holds = self.accounts_of(id).count();
        let owns = self.companies_of(id).count();
        let earns = self.series.iter().filter(|s| s.entity == EntityRef::Person(id)).count();
        if holds + owns + earns > 0 {
            return Err(EngineError::Insufficient(format!("holds {holds} account(s), owns {owns} company(ies), has {earns} series")));
        }
        self.people.retain(|p| p.id != id);
        self.policies.retain(|p| p.object != ObjectRef::Person(id));
        Ok(())
    }
}

fn validate_shares(shares: impl Iterator<Item = u32>) -> EngineResult<()> {
    let total: u32 = shares.sum();
    if total != 10_000 {
        return Err(EngineError::Insufficient(format!("ownership shares total {}% instead of 100%", total / 100)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};
    use crate::provenance::Disclosure;
    use crate::timeline::{AmountSpec, DateSpec, Recurrence};
    use crate::vocab::Certainty;
    use crate::Currency;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn account(id: u32, holder: Holder, currency: Currency) -> Account {
        Account {
            id: AccountId::new(id),
            name: format!("Account {id}"),
            institution: "Bank".into(),
            kind: AccountKind::Checking,
            holder,
            currency,
            liquidity: Liquidity::Immediate,
            minimum_balance: None,
            transfer_delay_days: 0,
            fees: Vec::new(),
            tax_treatment: String::new(),
            source_of_truth: SourceOfTruth::Manual,
            last_reconciled: None,
            withdrawals_permitted: true,
            funds_categories: Vec::new(),
            include_in_household: true,
            settled_balance: Money::from_major(100, currency),
            pending_balance: Money::zero(currency),
        }
    }

    #[test]
    fn every_created_object_gets_a_policy_and_is_visible_to_its_creator() {
        let mut household = Household::empty("Ours", Currency::USD, d(2026, 9, 11));
        let me = household.add_person(Person { id: household.next_person_id(), name: "Me".into(), role: HouseholdRole::Owner });
        let viewer = crate::authz::Viewer::person(me);
        assert_eq!(household.disclosure_for(viewer, ObjectRef::Person(me)), Disclosure::Full);
        let holder = Holder::Persons(vec![OwnershipShare { person: me, basis_points: 10_000 }]);
        let account = household.add_account(account(1, holder, Currency::USD), VisibilityPreset::FullyShared, CalculationAccess::Full).unwrap();
        assert_eq!(household.disclosure_for(viewer, ObjectRef::Account(account)), Disclosure::Full);
        let company = household
            .add_company(Company {
                id: household.next_company_id(),
                name: "Co".into(),
                jurisdiction: "—".into(),
                owners: vec![OwnershipShare { person: me, basis_points: 10_000 }],
                roles: Vec::new(),
                employees: Vec::new(),
                constraints: Vec::new(),
            })
            .unwrap();
        assert_eq!(household.disclosure_for(viewer, ObjectRef::Company(company)), Disclosure::Full);
        let scenario = household.add_scenario(Scenario { id: household.next_scenario_id(), name: "Private plan".into(), description: String::new(), private_to: Some(me) }, me);
        assert_eq!(household.disclosure_for(viewer, ObjectRef::Scenario(scenario)), Disclosure::Full);
        assert_eq!(household.policies.len(), 4);
    }

    #[test]
    fn shares_and_currencies_are_validated() {
        let mut household = fixtures::plan_household();
        let bad_shares = Holder::Persons(vec![OwnershipShare { person: ids::PERSON_A, basis_points: 6_000 }, OwnershipShare { person: ids::PERSON_B, basis_points: 6_000 }]);
        let err = household.add_account(account(50, bad_shares, Currency::PKR), VisibilityPreset::FullyShared, CalculationAccess::Full).unwrap_err();
        assert!(err.to_string().contains("120%"));
        let foreign = Holder::Persons(vec![OwnershipShare { person: ids::PERSON_A, basis_points: 10_000 }]);
        assert!(matches!(household.add_account(account(51, foreign, Currency::EUR), VisibilityPreset::FullyShared, CalculationAccess::Full), Err(EngineError::Money(_))));
        let unknown = Holder::Persons(vec![OwnershipShare { person: PersonId::new(99), basis_points: 10_000 }]);
        assert!(matches!(household.add_account(account(52, unknown, Currency::PKR), VisibilityPreset::FullyShared, CalculationAccess::Full), Err(EngineError::UnknownPerson(_))));
    }

    #[test]
    fn deletion_is_refused_while_referenced() {
        let mut household = fixtures::plan_household();
        let err = household.remove_account(ids::SHARED_SAVINGS).unwrap_err();
        assert!(err.to_string().contains("series"));
        let err = household.remove_person(ids::PERSON_A).unwrap_err();
        assert!(err.to_string().contains("holds"));
        // An unreferenced account can go, and its policy with it.
        let holder = Holder::Persons(vec![OwnershipShare { person: ids::PERSON_A, basis_points: 10_000 }]);
        let id = household.add_account(account(60, holder, Currency::PKR), VisibilityPreset::Private, CalculationAccess::Excluded).unwrap();
        household.remove_account(id).unwrap();
        assert!(household.policy_for(ObjectRef::Account(id)).is_none());
        // Removing a series drops its links and assumption references.
        household.remove_series(ids::CLIENT_RECEIVABLE).unwrap();
        assert!(household.links.iter().all(|l| l.series != ids::CLIENT_RECEIVABLE));
        assert!(household.assumptions.iter().all(|a| !a.applies_to.contains(&ids::CLIENT_RECEIVABLE)));
    }

    #[test]
    fn reconcile_moves_as_of_forward_and_actuals_link() {
        let mut household = fixtures::plan_household();
        household.reconcile_account(ids::PERSON_B_CHECKING, pkr(950_000), d(2026, 9, 15)).unwrap();
        assert_eq!(household.as_of, d(2026, 9, 15));
        assert_eq!(household.account(ids::PERSON_B_CHECKING).unwrap().settled_balance, pkr(950_000));
        let txn = household.add_actual(ActualTransaction { id: household.next_transaction_id(), date: d(2026, 9, 15), account: ids::PERSON_B_CHECKING, amount: pkr(-130_000), description: "Groceries etc.".into() }).unwrap();
        household.link_actual(ReconciliationLink { series: ids::OTHER_SPENDING, original_due: d(2026, 9, 15), transaction: txn, amount: pkr(130_000) }).unwrap();
        let series = household.series_by_id(ids::OTHER_SPENDING).unwrap();
        let occurrences = household.expand_series(series, d(2026, 9, 1), d(2026, 10, 1));
        assert_eq!(occurrences[0].status, crate::timeline::OccurrenceStatus::Fulfilled);
        assert!(household.link_actual(ReconciliationLink { series: SeriesId::new(999), original_due: d(2026, 9, 15), transaction: txn, amount: pkr(1) }).is_err());
    }

    #[test]
    fn series_validation_checks_accounts_and_currency() {
        let mut household = fixtures::plan_household();
        let mut series = EventSeries {
            id: household.next_series_id(),
            name: "Gym".into(),
            direction: Direction::Expense,
            amount: AmountSpec::Exact(pkr(8_000)),
            amount_changes: Vec::new(),
            exceptions: Vec::new(),
            recurrence: Recurrence::OneTime { on: DateSpec::Exact(d(2026, 10, 1)) },
            settlement_lag_days: 0,
            availability_lag_days: 0,
            intraday_order: 10,
            account: AccountId::new(404),
            linked_account: None,
            entity: EntityRef::Person(ids::PERSON_A),
            certainty: Certainty::UserEstimated,
            category: "Living".into(),
            tax_treatment: String::new(),
            scenario: None,
            notes: String::new(),
        };
        assert!(matches!(household.add_series(series.clone()), Err(EngineError::UnknownAccount(_))));
        series.account = ids::PERSON_A_CURRENT;
        household.add_series(series).unwrap();
        assert_eq!(household.series.last().unwrap().name, "Gym");
    }
}
