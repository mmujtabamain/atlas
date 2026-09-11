//! Money and balance definitions (§6), reservations (§17) and E01.
//!
//! One bank balance is never one number here. For an account we compute
//! *ledger cash*, *reserved cash* and *free current cash*, each as a
//! [`Calc`] whose chain names every earmark, states whether it is nested or
//! disjoint (§17, M02) and keeps negative headroom visible (§6.6). For the
//! household we aggregate only what may be aggregated: joint accounts once,
//! company cash never (§8.5), excluded objects shown as excluded.

use crate::authz::CalculationAccess;
use crate::ids::*;
use crate::model::{Account, Coverage, Holder, Household, Liquidity};
use crate::money::Money;
use crate::provenance::{Calc, ProvNode, ProvValue};
use crate::vocab::{Certainty, MoneyClass, ResultStrength};
use crate::{EngineError, EngineResult};

/// §6.4–§6.6 for one account.
#[derive(Clone, Debug)]
pub struct AccountLiquidity {
    pub account: AccountId,
    pub ledger_cash: Calc<Money>,
    pub pending: Money,
    pub reserved: Calc<Money>,
    pub free: Calc<Money>,
}

fn ledger_node(account: &Account) -> ProvNode {
    let reconciled = account
        .last_reconciled
        .map(|d| d.format("%d %b %Y").to_string())
        .unwrap_or_else(|| "never reconciled".into());
    ProvNode::input(
        format!("Settled ledger cash — {}", account.name),
        account.settled_balance,
        format!("{} balance, reconciled {reconciled}", account.source_of_truth.label().to_lowercase()),
    )
    .money_class(MoneyClass::ConfirmedCurrent)
    .certainty(Certainty::Confirmed)
    .subject(ObjectRef::Account(account.id))
}

/// Ledger, reserved and free cash of one account with full provenance.
pub fn account_liquidity(household: &Household, id: AccountId) -> EngineResult<AccountLiquidity> {
    let account = household.account(id).ok_or(EngineError::UnknownAccount(id))?;
    let currency = account.currency;
    let ledger = ledger_node(account);

    let active: Vec<_> = household.active_reservations_on(id).collect();
    let mut terms: Vec<ProvNode> = Vec::new();
    let mut reserved_total = Money::zero(currency);
    for reservation in &active {
        let nested_in = match reservation.coverage {
            Coverage::NestedIn(outer) => active.iter().find(|r| r.id == outer).map(|r| r.name.clone()),
            _ => None,
        };
        let node = match nested_in {
            Some(outer) => ProvNode::excluded(
                format!("{} earmark", reservation.name),
                reservation.amount,
                format!("nested inside the {outer} earmark; adds nothing on top (§17)"),
            ),
            None => {
                reserved_total = reserved_total.checked_add(reservation.amount)?;
                ProvNode::input(
                    format!("{} earmark", reservation.name),
                    reservation.amount,
                    reservation.purpose.clone(),
                )
                .note(reservation.hardness.label())
            }
        }
        .money_class(MoneyClass::ReservedCurrent)
        .certainty(Certainty::Confirmed)
        .subject(ObjectRef::Reservation(reservation.id));
        terms.push(node);
    }
    if let Some(minimum) = account.minimum_balance {
        let covering = active.iter().find(|r| r.coverage == Coverage::CoversAccountMinimum);
        let node = match covering {
            Some(reservation) => ProvNode::excluded(
                "Bank minimum balance",
                minimum,
                format!("already inside the {} earmark (§17)", reservation.name),
            ),
            None => {
                reserved_total = reserved_total.checked_add(minimum)?;
                ProvNode::input("Bank minimum balance", minimum, "account constraint (§17)").note("Hard constraint")
            }
        }
        .money_class(MoneyClass::ReservedCurrent)
        .subject(ObjectRef::Account(id));
        terms.push(node);
    }
    let reserved = ProvNode::sum(format!("Reserved cash — {}", account.name), reserved_total, terms)
        .money_class(MoneyClass::ReservedCurrent)
        .subject(ObjectRef::Account(id));

    let free_value = account.settled_balance.checked_sub(reserved_total)?;
    let mut free = ProvNode::sum(
        format!("Free current cash — {}", account.name),
        free_value,
        vec![ledger.clone(), reserved.clone().minus()],
    )
    .money_class(MoneyClass::FreeCurrent)
    .strength(ResultStrength::ExactAccounting)
    .subject(ObjectRef::Account(id))
    .note("Reservations constrain spendability; they are not cash postings (§11.1).");
    if free_value.is_negative() {
        free = free.note(format!(
            "Negative headroom: earmarks exceed settled cash by {} — kept signed, not floored (§6.6).",
            free_value.abs().format()
        ));
    }
    if !account.pending_balance.is_zero() {
        free = free.note(format!(
            "Pending, unsettled postings of {} are not included (§6.4).",
            account.pending_balance.format()
        ));
    }

    Ok(AccountLiquidity {
        account: id,
        ledger_cash: Calc::new(account.settled_balance, ledger),
        pending: account.pending_balance,
        reserved: Calc::new(reserved_total, reserved),
        free: Calc::new(free_value, free),
    })
}

/// §6 for the household boundary.
#[derive(Clone, Debug)]
pub struct HouseholdLiquidity {
    pub liquid_cash: Calc<Money>,
    pub reserved: Calc<Money>,
    pub free: Calc<Money>,
    pub total_assets: Calc<Money>,
    pub liabilities: Calc<Money>,
    pub net_worth: Calc<Money>,
    pub included: Vec<AccountId>,
    pub excluded: Vec<(AccountId, String)>,
}

/// Why an account stays out of the household boundary, if it does.
pub fn household_exclusion_reason(household: &Household, account: &Account) -> Option<String> {
    if account.is_company_account() {
        return Some("business cash is not household cash (§8.5)".into());
    }
    if !account.include_in_household {
        return Some("excluded from the household boundary by its inclusion status (§7)".into());
    }
    if household.calculation_access_for(ObjectRef::Account(account.id)) == CalculationAccess::Excluded {
        return Some("excluded by its access policy (§7.3)".into());
    }
    if account.currency != household.base_currency {
        return Some(format!(
            "held in {}; no conversion convention to {} is declared (§32.1)",
            account.currency, household.base_currency
        ));
    }
    None
}

fn joint_note(account: &Account) -> Option<String> {
    match &account.holder {
        Holder::Persons(shares) if shares.len() > 1 => Some(format!(
            "joint {} — counted once at 100% for the household (§7)",
            shares.iter().map(|s| format!("{}%", s.basis_points / 100)).collect::<Vec<_>>().join("/")
        )),
        _ => None,
    }
}

/// Liquid, reserved, free cash and net worth of the household with provenance.
pub fn household_liquidity(household: &Household) -> EngineResult<HouseholdLiquidity> {
    let currency = household.base_currency;
    let mut liquid_terms = Vec::new();
    let mut reserved_terms = Vec::new();
    let mut asset_terms = Vec::new();
    let mut liability_terms = Vec::new();
    let mut liquid_total = Money::zero(currency);
    let mut reserved_total = Money::zero(currency);
    let mut assets_total = Money::zero(currency);
    let mut liabilities_total = Money::zero(currency);
    let mut included = Vec::new();
    let mut excluded = Vec::new();

    for account in &household.accounts {
        if let Some(reason) = household_exclusion_reason(household, account) {
            let node = ProvNode::excluded(account.name.clone(), account.settled_balance, reason.clone())
                .subject(ObjectRef::Account(account.id));
            if account.kind.is_cash() {
                liquid_terms.push(node.clone());
            }
            asset_terms.push(node);
            excluded.push((account.id, reason));
            continue;
        }
        included.push(account.id);
        if account.kind.is_liability() {
            let owed = account.settled_balance.negated();
            liabilities_total = liabilities_total.checked_add(owed)?;
            liability_terms.push(
                ProvNode::input(account.name.clone(), owed, format!("{} balance owed", account.kind.label().to_lowercase()))
                    .money_class(MoneyClass::ConfirmedCurrent)
                    .certainty(Certainty::Confirmed)
                    .subject(ObjectRef::Account(account.id)),
            );
            continue;
        }
        assets_total = assets_total.checked_add(account.settled_balance)?;
        let mut asset_node = ledger_node(account);
        if let Some(note) = joint_note(account) {
            asset_node = asset_node.note(note);
        }
        let is_liquid = account.kind.is_cash() && account.liquidity == Liquidity::Immediate;
        if is_liquid {
            liquid_total = liquid_total.checked_add(account.settled_balance)?;
            liquid_terms.push(asset_node.clone());
            let per_account = account_liquidity(household, account.id)?;
            reserved_total = reserved_total.checked_add(per_account.reserved.money())?;
            reserved_terms.push(per_account.reserved.node().clone());
        } else {
            asset_node = asset_node.note(format!("not liquid: {} (§6.4)", account.liquidity.describe().to_lowercase()));
        }
        asset_terms.push(asset_node);
    }

    let liquid_node = ProvNode::sum("Household liquid cash", liquid_total, liquid_terms)
        .money_class(MoneyClass::ConfirmedCurrent)
        .note("Only settled, immediately accessible cash of included personal accounts (§6.4).");
    let reserved_node = ProvNode::sum("Household reserved cash", reserved_total, reserved_terms)
        .money_class(MoneyClass::ReservedCurrent)
        .note("Non-overlapping earmarks and bank minimums on the included accounts (§6.5, §17).");
    let free_total = liquid_total.checked_sub(reserved_total)?;
    let mut free_node = ProvNode::sum(
        "Household free current cash",
        free_total,
        vec![liquid_node.clone(), reserved_node.clone().minus()],
    )
    .money_class(MoneyClass::FreeCurrent)
    .note("Present and not reserved (§6.6). Nested minimums were not deducted twice.");
    if free_total.is_negative() {
        free_node = free_node.note(format!(
            "Negative headroom of {} — kept signed as a warning (§6.6).",
            free_total.abs().format()
        ));
    }
    let assets_node = ProvNode::sum("Total assets", assets_total, asset_terms)
        .money_class(MoneyClass::ConfirmedCurrent)
        .note("Everything in the planning boundary with positive value (§6.1); company equity is not added (M04).");
    let liabilities_node = ProvNode::sum("Liabilities", liabilities_total, liability_terms)
        .money_class(MoneyClass::ConfirmedCurrent)
        .note("Everything owed (§6.2).");
    let net_worth_total = assets_total.checked_sub(liabilities_total)?;
    let net_worth_node = ProvNode::sum(
        "Net worth",
        net_worth_total,
        vec![assets_node.clone(), liabilities_node.clone().minus()],
    )
    .money_class(MoneyClass::ConfirmedCurrent)
    .note("Assets minus liabilities (§6.3).");

    Ok(HouseholdLiquidity {
        liquid_cash: Calc::new(liquid_total, liquid_node),
        reserved: Calc::new(reserved_total, reserved_node),
        free: Calc::new(free_total, free_node),
        total_assets: Calc::new(assets_total, assets_node),
        liabilities: Calc::new(liabilities_total, liabilities_node),
        net_worth: Calc::new(net_worth_total, net_worth_node),
        included,
        excluded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn e01_reserving_money_is_not_spending_it() {
        let mut household = fixtures::plan_household();
        let shared = fixtures::ids::SHARED_SAVINGS;
        let before = account_liquidity(&household, shared).unwrap();
        assert_eq!(before.ledger_cash.money(), fixtures::pkr(2_000_000));
        assert_eq!(before.reserved.money(), fixtures::pkr(1_350_000));
        assert_eq!(before.free.money(), fixtures::pkr(650_000));
        assert!(before.free.node().verify_sums().is_empty());

        // Pay the 300,000 tax bill and release the matching earmark (E01).
        let tax = fixtures::ids::TAX_RESERVE;
        household.reservations.iter_mut().find(|r| r.id == tax).unwrap().released_on = Some(household.as_of);
        household.accounts.iter_mut().find(|a| a.id == shared).unwrap().settled_balance = fixtures::pkr(1_700_000);
        let after = account_liquidity(&household, shared).unwrap();
        assert_eq!(after.ledger_cash.money(), fixtures::pkr(1_700_000));
        assert_eq!(after.reserved.money(), fixtures::pkr(1_050_000));
        assert_eq!(after.free.money(), fixtures::pkr(650_000), "the paid liability must not reduce spendability twice");
    }

    #[test]
    fn nested_floors_are_not_deducted_twice() {
        let household = fixtures::plan_household();
        let liquidity = account_liquidity(&household, fixtures::ids::PERSON_A_SAVINGS).unwrap();
        // 1,500,000 settled; 300,000 bank minimum is inside the 400,000 buffer reservation.
        assert_eq!(liquidity.reserved.money(), fixtures::pkr(400_000));
        assert_eq!(liquidity.free.money(), fixtures::pkr(1_100_000));
        let text = liquidity.free.node().render_chain();
        assert!(text.contains("(excluded) Bank minimum balance"));
    }

    #[test]
    fn household_free_cash_excludes_company_cash_and_counts_joint_once() {
        let household = fixtures::plan_household();
        let liquidity = household_liquidity(&household).unwrap();
        // Shared savings 2,000,000 (joint, once) + A savings 1,500,000 + B checking 900,000.
        assert_eq!(liquidity.liquid_cash.money(), fixtures::pkr(4_400_000));
        assert_eq!(liquidity.reserved.money(), fixtures::pkr(1_350_000 + 400_000));
        assert_eq!(liquidity.free.money(), fixtures::pkr(4_400_000 - 1_750_000));
        assert!(liquidity.excluded.iter().any(|(id, reason)| *id == fixtures::ids::ALPHA_OPERATING && reason.contains("§8.5")));
        let text = liquidity.liquid_cash.node().render_chain();
        assert!(text.contains("(excluded) Company Alpha operating"));
        assert!(text.contains("counted once at 100%"));
        for calc in [&liquidity.liquid_cash, &liquidity.free, &liquidity.total_assets, &liquidity.net_worth] {
            assert!(calc.node().verify_sums().is_empty(), "{}", calc.node().label());
        }
        // Net worth = assets (liquid 4,400,000 + fixed deposit 1,000,000) − Visa 85,000.
        assert_eq!(liquidity.total_assets.money(), fixtures::pkr(5_400_000));
        assert_eq!(liquidity.liabilities.money(), fixtures::pkr(85_000));
        assert_eq!(liquidity.net_worth.money(), fixtures::pkr(5_315_000));
    }

    #[test]
    fn negative_headroom_stays_visible() {
        let mut household = fixtures::plan_household();
        household.accounts.iter_mut().find(|a| a.id == fixtures::ids::SHARED_SAVINGS).unwrap().settled_balance = fixtures::pkr(1_000_000);
        let liquidity = account_liquidity(&household, fixtures::ids::SHARED_SAVINGS).unwrap();
        assert_eq!(liquidity.free.money(), fixtures::pkr(-350_000));
        assert!(liquidity.free.node().notes().iter().any(|n| n.contains("Negative headroom")));
        assert_eq!(liquidity.free.money().clamped_at_zero(), fixtures::pkr(0));
    }
}

// ----- companies (§8, E07) ------------------------------------------------------

/// A company's cash position with the plan's caveat: a cash-constraint
/// ceiling is **not** lawfully distributable cash (E07, M27).
#[derive(Clone, Debug)]
pub struct CompanyCash {
    pub company: CompanyId,
    /// Settled cash across the company's accounts (§6.8 business cash).
    pub cash: Calc<Money>,
    /// Earmarks and constraints that must stay funded (§8.6).
    pub committed: Calc<Money>,
    /// `cash − committed`: the ceiling before any extraction costs.
    pub ceiling: Calc<Money>,
    /// What the household may lawfully extract: unresolved until routes are
    /// modelled (M9/M27), and never inferred from the ceiling.
    pub extractable: Calc<Money>,
}

/// Business cash, committed obligations and the cash-constraint ceiling of a
/// company, each with its chain.
pub fn company_cash(household: &Household, id: CompanyId) -> EngineResult<CompanyCash> {
    let company = household.company(id).ok_or(EngineError::UnknownCompany(id))?;
    let currency = household.base_currency;
    let mut cash_terms = Vec::new();
    let mut cash_total = Money::zero(currency);
    let mut committed_terms = Vec::new();
    let mut committed_total = Money::zero(currency);

    for account in household.company_accounts(id) {
        if account.currency != currency {
            cash_terms.push(
                ProvNode::excluded(account.name.clone(), account.settled_balance, format!("held in {}; no conversion convention declared (§32.1)", account.currency))
                    .subject(ObjectRef::Account(account.id)),
            );
            continue;
        }
        cash_total = cash_total.checked_add(account.settled_balance)?;
        cash_terms.push(ledger_node(account));
        for reservation in household.active_reservations_on(account.id) {
            committed_total = committed_total.checked_add(reservation.amount)?;
            committed_terms.push(
                ProvNode::input(format!("{} ({})", reservation.name, account.name), reservation.amount, reservation.purpose.clone())
                    .money_class(MoneyClass::ReservedCurrent)
                    .note(reservation.hardness.label())
                    .subject(ObjectRef::Reservation(reservation.id)),
            );
        }
    }
    for constraint in &company.constraints {
        match constraint {
            crate::model::CompanyConstraint::MinimumWorkingCapital(floor) => {
                committed_total = committed_total.checked_add(*floor)?;
                committed_terms.push(
                    ProvNode::input("Working-capital floor", *floor, constraint.describe())
                        .money_class(MoneyClass::ReservedCurrent)
                        .note("Hard constraint (§8.6)")
                        .subject(ObjectRef::Company(id)),
                );
            }
            crate::model::CompanyConstraint::TaxReserve(reserve) => {
                committed_total = committed_total.checked_add(*reserve)?;
                committed_terms.push(
                    ProvNode::input("Tax reserve", *reserve, constraint.describe())
                        .money_class(MoneyClass::ReservedCurrent)
                        .note("Hard constraint (§8.6)")
                        .subject(ObjectRef::Company(id)),
                );
            }
            other => committed_terms.push(
                ProvNode::excluded(other.describe(), ProvValue::Empty, "a timing or payroll-coverage rule, not a cash amount; evaluated by the funding engine (M9)")
                    .subject(ObjectRef::Company(id)),
            ),
        }
    }

    let cash_node = ProvNode::sum(format!("{} business cash", company.name), cash_total, cash_terms)
        .money_class(MoneyClass::ConfirmedCurrent)
        .certainty(Certainty::Confirmed)
        .subject(ObjectRef::Company(id))
        .note("Business cash is not household cash and never enters household free cash (§8.5).");
    let committed_node = ProvNode::sum(format!("{} committed obligations", company.name), committed_total, committed_terms)
        .money_class(MoneyClass::ReservedCurrent)
        .subject(ObjectRef::Company(id))
        .note("Disjoint earmarks and constraint floors; treated as constraints, not suggestions (§8.6).");
    let ceiling_total = cash_total.checked_sub(committed_total)?;
    let ceiling_node = ProvNode::sum(
        format!("{} cash-constraint ceiling", company.name),
        ceiling_total,
        vec![cash_node.clone(), committed_node.clone().minus()],
    )
    .money_class(MoneyClass::FreeCurrent)
    .strength(ResultStrength::ExactAccounting)
    .subject(ObjectRef::Company(id))
    .note("Before any extraction costs. A simple disjoint cash constraint, not a statement of what may lawfully leave the company (E07).");
    let extractable_node = ProvNode::formula(
        format!("{} lawfully extractable cash", company.name),
        ProvValue::Text("not yet determinable".into()),
        "min over eligible routes m of NetReceipt(x_m) subject to payroll, working capital, taxes and distribution authority (M27)",
        vec![ceiling_node.clone()],
    )
    .strength(ResultStrength::Unresolved)
    .subject(ObjectRef::Company(id))
    .note("Each route (salary, permitted dividend, documented reimbursement, genuine shareholder-loan repayment) must be modelled independently with its legal capacity; book cash does not prove distributable profit (M27). Arrives with M9.");

    Ok(CompanyCash {
        company: id,
        cash: Calc::new(cash_total, cash_node),
        committed: Calc::new(committed_total, committed_node),
        ceiling: Calc::new(ceiling_total, ceiling_node),
        extractable: Calc::new(Money::zero(currency), extractable_node),
    })
}

// ----- people (§7 attribution) ----------------------------------------------------

/// The economic share of held accounts attributed to one person (§7:
/// "Individual views can attribute the appropriate economic share"). Never
/// used for household aggregation, which counts a joint account once.
pub fn person_attribution(household: &Household, person: PersonId) -> EngineResult<Calc<Money>> {
    household.person(person).ok_or(EngineError::UnknownPerson(person))?;
    let currency = household.base_currency;
    let mut terms = Vec::new();
    let mut total = Money::zero(currency);
    for account in household.accounts_of(person) {
        let share = account.holder.share_of(person);
        if account.currency != currency {
            terms.push(
                ProvNode::excluded(account.name.clone(), account.settled_balance, format!("held in {}; no conversion convention declared (§32.1)", account.currency))
                    .subject(ObjectRef::Account(account.id)),
            );
            continue;
        }
        let attributed = account.settled_balance.share_basis_points(share);
        total = total.checked_add(attributed)?;
        terms.push(
            ProvNode::formula(
                format!("{} ({}% of {})", account.name, share / 100, account.settled_balance.format()),
                attributed,
                format!("settled balance × {share} basis points, rounded half away from zero"),
                Vec::new(),
            )
            .money_class(MoneyClass::ConfirmedCurrent)
            .certainty(Certainty::Confirmed)
            .subject(ObjectRef::Account(account.id)),
        );
    }
    let name = household.entity_name(EntityRef::Person(person));
    let node = ProvNode::sum(format!("{name} — attributed share of held accounts"), total, terms)
        .money_class(MoneyClass::ConfirmedCurrent)
        .subject(ObjectRef::Person(person))
        .note("Economic attribution only; the household view counts each joint account once at 100% (§7). Liabilities are attributed with their negative sign.");
    Ok(Calc::new(total, node))
}

#[cfg(test)]
mod entity_tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};

    #[test]
    fn e07_company_cash_ceiling_is_not_distributable_cash() {
        let household = fixtures::plan_household();
        let alpha = company_cash(&household, ids::ALPHA).unwrap();
        // 2,000,000 operating + 350,000 payroll account; committed: 700,000 payroll + 200,000 tax + 600,000 buffer.
        assert_eq!(alpha.cash.money(), pkr(2_350_000));
        assert_eq!(alpha.committed.money(), pkr(1_500_000));
        assert_eq!(alpha.ceiling.money(), pkr(850_000));
        assert_eq!(alpha.extractable.node().result_strength(), ResultStrength::Unresolved);
        assert!(alpha.ceiling.node().verify_sums().is_empty());
        let text = alpha.ceiling.node().render_chain();
        assert!(text.contains("Working-capital floor"));
        assert!(text.contains("(excluded) Keep at least 3 months of payroll"));
    }

    #[test]
    fn joint_account_is_attributed_by_share_but_counted_once_for_the_household() {
        let household = fixtures::plan_household();
        let a = person_attribution(&household, ids::PERSON_A).unwrap();
        let b = person_attribution(&household, ids::PERSON_B).unwrap();
        // A: 50% of 2,000,000 + 1,500,000 + (−85,000) + 50% of 1,000,000.
        assert_eq!(a.money(), pkr(1_000_000 + 1_500_000 - 85_000 + 500_000));
        // B: 50% of 2,000,000 + 900,000 + 50% of 1,000,000.
        assert_eq!(b.money(), pkr(1_000_000 + 900_000 + 500_000));
        let household_liquid = household_liquidity(&household).unwrap();
        // The household counts the shared savings once: attribution shares sum to it, not twice it.
        assert_eq!(household_liquid.liquid_cash.money(), pkr(4_400_000));
        assert!(a.node().verify_sums().is_empty() && b.node().verify_sums().is_empty());
    }
}
