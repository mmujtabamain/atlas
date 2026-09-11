//! The fictitious plan household.
//!
//! Every number here comes from `plan.md`'s own examples so screens can be
//! checked against the document by eye: E01 (2,000,000 shared savings with
//! 800,000 / 300,000 / 250,000 earmarks → 650,000 free), §13.1 (Account A
//! 1,500,000, Account B 900,000, Company Alpha and Beta), E07 (Alpha
//! 2,000,000 cash, 700,000 payroll, 200,000 tax remittance, 600,000 buffer),
//! §10.2 (the assumption list), §14.3–§14.4 (the DEMO tax rules), §7.5 (the
//! private account policy). Nothing is real finance or real law.

use crate::authz::{AccessPolicy, CalculationAccess, Grantees, VisibilityPreset};
use crate::ids::*;
use crate::model::*;
use crate::money::{Currency, Money};
use crate::provenance::Disclosure;
use crate::rules::{Condition, Rule, RuleAction, RuleScope, RuleVersion, Trigger};
use crate::timeline::*;
use crate::vocab::Certainty;
use chrono::{NaiveDate, NaiveDateTime};

/// Stable ids of the fixture objects, for tests and screenshots.
pub mod ids {
    use crate::ids::*;

    pub const PERSON_A: PersonId = PersonId::new(1);
    pub const PERSON_B: PersonId = PersonId::new(2);

    pub const ALPHA: CompanyId = CompanyId::new(1);
    pub const BETA: CompanyId = CompanyId::new(2);

    pub const SHARED_SAVINGS: AccountId = AccountId::new(1);
    pub const PERSON_A_CURRENT: AccountId = AccountId::new(2);
    pub const PERSON_B_CHECKING: AccountId = AccountId::new(3);
    pub const PERSON_A_VISA: AccountId = AccountId::new(4);
    pub const FIXED_DEPOSIT: AccountId = AccountId::new(5);
    pub const ALPHA_OPERATING: AccountId = AccountId::new(6);
    pub const ALPHA_PAYROLL: AccountId = AccountId::new(7);
    pub const BETA_OPERATING: AccountId = AccountId::new(8);
    pub const CASH_WALLET: AccountId = AccountId::new(9);
    /// Kept for the E01 test name; the account is the shared savings account.
    pub const PERSON_A_SAVINGS: AccountId = PERSON_A_CURRENT;

    pub const EMERGENCY_RESERVE: ReservationId = ReservationId::new(1);
    pub const TAX_RESERVE: ReservationId = ReservationId::new(2);
    pub const SCHOOL_FEE_RESERVE: ReservationId = ReservationId::new(3);
    pub const A_LIQUIDITY_BUFFER: ReservationId = ReservationId::new(4);
    pub const ALPHA_COMMITTED_PAYROLL: ReservationId = ReservationId::new(5);
    pub const ALPHA_TAX_REMITTANCE: ReservationId = ReservationId::new(6);

    pub const SALARY_A: SeriesId = SeriesId::new(1);
    pub const SALARY_B: SeriesId = SeriesId::new(2);
    pub const FREELANCE: SeriesId = SeriesId::new(3);
    pub const CLIENT_RECEIVABLE: SeriesId = SeriesId::new(4);
    pub const RENT: SeriesId = SeriesId::new(5);
    pub const OTHER_SPENDING: SeriesId = SeriesId::new(6);
    pub const SCHOOL_FEES: SeriesId = SeriesId::new(7);
    pub const CAR_DOWN_PAYMENT: SeriesId = SeriesId::new(8);
    pub const ALPHA_REVENUE: SeriesId = SeriesId::new(9);
    pub const ALPHA_PAYROLL_RUN: SeriesId = SeriesId::new(10);
    pub const VISA_SETTLEMENT: SeriesId = SeriesId::new(11);
    pub const CAR_INSURANCE: SeriesId = SeriesId::new(12);
    pub const CARD_PURCHASES: SeriesId = SeriesId::new(13);
    pub const ATM_WITHDRAWAL: SeriesId = SeriesId::new(14);
    pub const FOREIGN_SUBSCRIPTION: SeriesId = SeriesId::new(15);

    pub const TXN_INSURANCE: TransactionId = TransactionId::new(1);
    pub const TXN_RECEIVABLE_PART: TransactionId = TransactionId::new(2);

    pub const BUY_CAR: ScenarioId = ScenarioId::new(1);
    pub const LEAVE_JOB: ScenarioId = ScenarioId::new(2);
    pub const LEAVE_JOB_AND_CAR: ScenarioId = ScenarioId::new(3);

    pub const RULE_FOREIGN_FEE: RuleId = RuleId::new(1);
    pub const RULE_FOREIGN_FEE_WAIVER: RuleId = RuleId::new(2);
    pub const RULE_TRANSFER_FEE: RuleId = RuleId::new(3);
    pub const RULE_FUND_FORBID_ALPHA: RuleId = RuleId::new(4);
    pub const RULE_FUND_SHARED: RuleId = RuleId::new(5);
    pub const RULE_FUND_A_CURRENT: RuleId = RuleId::new(6);
    pub const RULE_BANK_SELECTION: RuleId = RuleId::new(7);
}

/// Whole PKR, the fixture currency.
pub fn pkr(major: i64) -> Money {
    Money::from_major(major, Currency::PKR)
}

fn d(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("fixture date")
}

/// The date the fixture balances are reconciled to.
pub fn as_of() -> NaiveDate {
    d(2026, 9, 11)
}

/// The default forecast horizon used by the UI (E03 runs to 31 Jan 2027).
pub fn default_horizon() -> NaiveDate {
    d(2027, 1, 31)
}

fn policy_changed_at() -> NaiveDateTime {
    d(2026, 9, 10).and_hms_opt(18, 3, 0).expect("fixture time")
}

#[allow(clippy::too_many_arguments)]
fn account(
    id: AccountId,
    name: &str,
    institution: &str,
    kind: AccountKind,
    holder: Holder,
    settled: Money,
    liquidity: Liquidity,
    source: SourceOfTruth,
) -> Account {
    Account {
        id,
        name: name.into(),
        institution: institution.into(),
        kind,
        holder,
        currency: Currency::PKR,
        liquidity,
        minimum_balance: None,
        transfer_delay_days: 0,
        fees: Vec::new(),
        tax_treatment: "Profit on balances taxable under the DEMO pack".into(),
        source_of_truth: source,
        last_reconciled: Some(as_of()),
        withdrawals_permitted: true,
        funds_categories: Vec::new(),
        include_in_household: true,
        settled_balance: settled,
        pending_balance: Money::zero(Currency::PKR),
    }
}

fn reservation(id: ReservationId, name: &str, account: AccountId, amount: Money, coverage: Coverage, hardness: Hardness, purpose: &str) -> Reservation {
    Reservation {
        id,
        name: name.into(),
        account,
        amount,
        coverage,
        hardness,
        purpose: purpose.into(),
        released_on: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn series(
    id: SeriesId,
    name: &str,
    direction: Direction,
    amount: AmountSpec,
    recurrence: Recurrence,
    account: AccountId,
    entity: EntityRef,
    certainty: Certainty,
    category: &str,
) -> EventSeries {
    EventSeries {
        id,
        name: name.into(),
        direction,
        amount,
        amount_changes: Vec::new(),
        exceptions: Vec::new(),
        recurrence,
        // §11.1: expenses post before incomes on the same day unless a series says otherwise.
        intraday_order: match direction {
            Direction::Expense | Direction::Transfer { .. } => 10,
            Direction::Income => 20,
        },
        settlement_lag_days: 0,
        availability_lag_days: 0,
        account,
        linked_account: None,
        entity,
        certainty,
        category: category.into(),
        tax_treatment: String::new(),
        scenario: None,
        notes: String::new(),
    }
}

fn preset_policy(id: u32, object: ObjectRef, owners: Vec<PersonId>, preset: VisibilityPreset, access: CalculationAccess) -> AccessPolicy {
    AccessPolicy::preset(PolicyId::new(id), object, owners, preset, access, d(2026, 9, 1), policy_changed_at())
}

/// §14.3–§14.6 example rules: a foreign-card fee with a more specific promo
/// waiver (a visible conflict, §14.7), a fixed transfer fee, the car-purchase
/// funding order and a bank-selection rule.
fn demo_rules_for(_owner: PersonId) -> Vec<Rule> {
    use ids::*;
    let rule = |id: RuleId, name: &str, scope: RuleScope, trigger: Trigger, conditions: Vec<Condition>, action: RuleAction, priority: i32, from: NaiveDate, to: Option<NaiveDate>, scenario: Option<ScenarioId>, explanation: &str| Rule {
        id,
        name: name.into(),
        scope,
        trigger,
        conditions,
        action,
        priority,
        effective_from: from,
        effective_to: to,
        enabled: true,
        scenario,
        explanation: explanation.into(),
        version: 1,
        history: vec![RuleVersion { version: 1, changed_on: d(2026, 9, 1), summary: "created".into() }],
    };
    vec![
        rule(RULE_FOREIGN_FEE, "DEMO foreign card fee", RuleScope::Category("Foreign card".into()), Trigger::Expense, Vec::new(), RuleAction::AddFee { basis_points: 150, fixed: None, label: "foreign transaction fee".into() }, 10, d(2026, 7, 1), None, None, "§14.4: the bank's 1.5% fee is a separate fee event next to the 5% tax event."),
        rule(RULE_FOREIGN_FEE_WAIVER, "Visa promo: foreign fee waived Oct–Nov", RuleScope::Account(PERSON_A_VISA), Trigger::Expense, vec![Condition::CategoryIs("Foreign card".into())], RuleAction::AddFee { basis_points: 0, fixed: None, label: "promo waiver".into() }, 10, d(2026, 10, 1), Some(d(2026, 11, 30)), None, "Same priority as the category fee; wins on scope specificity while effective (§14.7)."),
        rule(RULE_TRANSFER_FEE, "Bank B transfer fee", RuleScope::Account(PERSON_A_CURRENT), Trigger::Transfer, vec![Condition::AmountAbove(pkr(50_000))], RuleAction::AddFee { basis_points: 0, fixed: Some(pkr(250)), label: "transfer fee".into() }, 5, d(2026, 1, 1), None, None, "A fixed 250 fee on transfers above 50,000 leaving the account."),
        rule(RULE_FUND_FORBID_ALPHA, "Car purchase funding: not Company Alpha before Dec 1", RuleScope::Scenario(BUY_CAR), Trigger::Funding, Vec::new(), RuleAction::ForbidAccount { account: ALPHA_OPERATING, unless_after: Some(d(2026, 12, 1)) }, 40, d(2026, 9, 1), None, Some(BUY_CAR), "§14.5 step 3."),
        rule(RULE_FUND_SHARED, "Car purchase funding: shared savings first", RuleScope::Scenario(BUY_CAR), Trigger::Funding, Vec::new(), RuleAction::PreferAccount { account: SHARED_SAVINGS, preserve: Some(pkr(1_000_000)) }, 30, d(2026, 9, 1), None, Some(BUY_CAR), "§14.5 step 1: use shared savings while preserving 1,000,000."),
        rule(RULE_FUND_A_CURRENT, "Car purchase funding: then Person A current", RuleScope::Scenario(BUY_CAR), Trigger::Funding, Vec::new(), RuleAction::PreferAccount { account: PERSON_A_CURRENT, preserve: Some(pkr(300_000)) }, 20, d(2026, 9, 1), None, Some(BUY_CAR), "§14.5 step 2: then Person A's account while preserving 300,000."),
        rule(RULE_BANK_SELECTION, "Ordinary expenses: Person B checking first", RuleScope::Category("Living".into()), Trigger::Expense, Vec::new(), RuleAction::BankSelection { prefer: PERSON_B_CHECKING, fallback: SHARED_SAVINGS, when_below: pkr(100_000) }, 10, d(2026, 1, 1), None, None, "§14.6: use shared savings only when B checking would fall below 100,000."),
    ]
}

/// Builds the fixture household.
pub fn plan_household() -> Household {
    use ids::*;
    let a = PERSON_A;
    let b = PERSON_B;
    let joint = Holder::Persons(vec![
        OwnershipShare { person: a, basis_points: 5_000 },
        OwnershipShare { person: b, basis_points: 5_000 },
    ]);
    let only = |p: PersonId| Holder::Persons(vec![OwnershipShare { person: p, basis_points: 10_000 }]);

    let people = vec![
        Person { id: a, name: "Person A".into(), role: HouseholdRole::Owner },
        Person { id: b, name: "Person B".into(), role: HouseholdRole::Member },
    ];

    let companies = vec![
        Company {
            id: ALPHA,
            name: "Company Alpha".into(),
            jurisdiction: "DEMO jurisdiction (not inferred; fictitious)".into(),
            owners: vec![OwnershipShare { person: a, basis_points: 10_000 }],
            roles: vec![EntityRoleAssignment { person: a, role: EntityRole::OwnerDirector }],
            employees: vec![
                Employee { name: "Person A (owner salary)".into(), person: Some(a), monthly_gross: pkr(500_000), start: d(2024, 1, 1), end: None },
                Employee { name: "Employee #42".into(), person: None, monthly_gross: pkr(350_000), start: d(2025, 3, 1), end: None },
                Employee { name: "Employee #43".into(), person: None, monthly_gross: pkr(350_000), start: d(2025, 9, 1), end: Some(d(2026, 11, 30)) },
            ],
            constraints: vec![
                CompanyConstraint::MinimumWorkingCapital(pkr(600_000)),
                CompanyConstraint::PayrollMonthsReserve(3),
                CompanyConstraint::NoDistributionBefore(d(2026, 12, 1)),
            ],
        },
        Company {
            id: BETA,
            name: "Company Beta".into(),
            jurisdiction: "DEMO jurisdiction (not inferred; fictitious)".into(),
            owners: vec![OwnershipShare { person: a, basis_points: 10_000 }],
            roles: vec![EntityRoleAssignment { person: a, role: EntityRole::OwnerDirector }],
            employees: Vec::new(),
            constraints: vec![CompanyConstraint::MinimumWorkingCapital(pkr(2_000_000))],
        },
    ];

    let mut person_a_current = account(
        PERSON_A_CURRENT,
        "Person A current account",
        "Bank B",
        AccountKind::Checking,
        only(a),
        pkr(1_500_000),
        Liquidity::Immediate,
        SourceOfTruth::Imported,
    );
    person_a_current.minimum_balance = Some(pkr(300_000));
    person_a_current.fees = vec![Fee { description: "Cash withdrawal above 50,000".into(), fixed: None, basis_points: 25 }];
    person_a_current.tax_treatment = "DEMO cash withdrawal withholding applies (§14.3)".into();

    let mut visa = account(
        PERSON_A_VISA,
        "Person A Visa card",
        "Bank A",
        AccountKind::CreditCard,
        only(a),
        pkr(-85_000),
        Liquidity::Immediate,
        SourceOfTruth::Synchronized,
    );
    visa.tax_treatment = "DEMO foreign card tax on non-PKR purchases (§14.4)".into();
    visa.withdrawals_permitted = false;

    let mut fixed_deposit = account(
        FIXED_DEPOSIT,
        "Household fixed deposit",
        "Bank C",
        AccountKind::FixedDeposit,
        joint.clone(),
        pkr(1_000_000),
        Liquidity::LockedUntil(d(2027, 3, 31)),
        SourceOfTruth::Manual,
    );
    fixed_deposit.withdrawals_permitted = false;
    fixed_deposit.fees = vec![Fee { description: "Early withdrawal penalty".into(), fixed: None, basis_points: 200 }];

    let mut alpha_operating = account(
        ALPHA_OPERATING,
        "Company Alpha operating",
        "Bank A Business",
        AccountKind::CompanyOperating,
        Holder::Company(ALPHA),
        pkr(2_000_000),
        Liquidity::Immediate,
        SourceOfTruth::Imported,
    );
    alpha_operating.include_in_household = false;
    alpha_operating.tax_treatment = "Corporate tax estimate (DEMO pack)".into();

    let mut alpha_payroll = account(
        ALPHA_PAYROLL,
        "Company Alpha payroll",
        "Bank A Business",
        AccountKind::CompanyPayroll,
        Holder::Company(ALPHA),
        pkr(350_000),
        Liquidity::Immediate,
        SourceOfTruth::Imported,
    );
    alpha_payroll.include_in_household = false;

    let mut beta_operating = account(
        BETA_OPERATING,
        "Company Beta operating",
        "Bank D Business",
        AccountKind::CompanyOperating,
        Holder::Company(BETA),
        pkr(2_000_000),
        Liquidity::Immediate,
        SourceOfTruth::Manual,
    );
    beta_operating.include_in_household = false;

    let mut cash_wallet = account(CASH_WALLET, "Cash wallet", "—", AccountKind::CashWallet, only(a), pkr(0), Liquidity::Immediate, SourceOfTruth::Manual);
    cash_wallet.tax_treatment = "None".into();

    let accounts = vec![
        account(
            SHARED_SAVINGS,
            "Shared savings",
            "Bank A",
            AccountKind::Savings,
            joint.clone(),
            pkr(2_000_000),
            Liquidity::Immediate,
            SourceOfTruth::Manual,
        ),
        person_a_current,
        account(
            PERSON_B_CHECKING,
            "Person B checking",
            "Bank A",
            AccountKind::Checking,
            only(b),
            pkr(900_000),
            Liquidity::Immediate,
            SourceOfTruth::Synchronized,
        ),
        visa,
        fixed_deposit,
        alpha_operating,
        alpha_payroll,
        beta_operating,
        cash_wallet,
    ];

    let reservations = vec![
        reservation(EMERGENCY_RESERVE, "Emergency reserve", SHARED_SAVINGS, pkr(800_000), Coverage::Disjoint, Hardness::Hard, "Household emergency fund (§17)"),
        reservation(TAX_RESERVE, "Tax reserve", SHARED_SAVINGS, pkr(300_000), Coverage::Disjoint, Hardness::Hard, "Tax incurred, payable later (§12.4)"),
        reservation(SCHOOL_FEE_RESERVE, "School fee reserve", SHARED_SAVINGS, pkr(250_000), Coverage::Disjoint, Hardness::SoftUserRelaxable, "December school fees"),
        reservation(A_LIQUIDITY_BUFFER, "Liquidity buffer", PERSON_A_CURRENT, pkr(400_000), Coverage::CoversAccountMinimum, Hardness::SoftUserRelaxable, "Includes the 300,000 bank minimum (§17)"),
        reservation(ALPHA_COMMITTED_PAYROLL, "Committed payroll", ALPHA_OPERATING, pkr(700_000), Coverage::Disjoint, Hardness::Hard, "Next payroll run for employees #42 and #43 (E07)"),
        reservation(ALPHA_TAX_REMITTANCE, "Tax remittance", ALPHA_OPERATING, pkr(200_000), Coverage::Disjoint, Hardness::Hard, "Withholding remittance due (E07)"),
    ];

    let mut salary_a = series(
        SALARY_A,
        "Person A salary from Company Alpha",
        Direction::Income,
        AmountSpec::Range { low: pkr(480_000), expected: pkr(500_000), high: pkr(520_000) },
        Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Date(d(2026, 11, 30)) },
        PERSON_A_CURRENT,
        EntityRef::Person(a),
        Certainty::Contractual,
        "Salary",
    );
    salary_a.linked_account = Some(ALPHA_PAYROLL);
    salary_a.settlement_lag_days = 1;
    salary_a.intraday_order = 20;
    salary_a.tax_treatment = "Salary income tax (DEMO pack)".into();
    salary_a.notes = "One linked movement: −500,000 on Company Alpha payroll, +500,000 for Person A (§8.4); the employee payroll run is a separate series.".into();

    let mut car_down_payment = series(
        CAR_DOWN_PAYMENT,
        "Car down payment",
        Direction::Expense,
        AmountSpec::Exact(pkr(2_500_000)),
        Recurrence::OneTime { on: DateSpec::Exact(d(2026, 11, 15)) },
        SHARED_SAVINGS,
        EntityRef::Household,
        Certainty::ScenarioOnly,
        "Major purchase",
    );
    car_down_payment.scenario = Some(BUY_CAR);

    let mut receivable = series(
        CLIENT_RECEIVABLE,
        "Client receivable — invoice 2026-031",
        Direction::Income,
        AmountSpec::Exact(pkr(350_000)),
        Recurrence::OneTime { on: DateSpec::Range { earliest: d(2026, 11, 1), expected: d(2026, 11, 10), latest: d(2026, 11, 15) } },
        PERSON_A_CURRENT,
        EntityRef::Person(a),
        Certainty::Expected,
        "Receivable",
    );
    receivable.settlement_lag_days = 2;
    receivable.notes = "150,000 already received on 9 Sep (partial); the remainder is expected by 15 Nov (§16).".into();

    let mut card_purchases = series(
        CARD_PURCHASES,
        "Card purchases",
        Direction::Expense,
        AmountSpec::Range { low: pkr(40_000), expected: pkr(60_000), high: pkr(90_000) },
        Recurrence::Monthly { every_n_months: 1, day: 20, from: d(2026, 9, 20), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
        PERSON_A_VISA,
        EntityRef::Person(a),
        Certainty::UserEstimated,
        "Card spending",
    );
    card_purchases.notes = "Expense recognition on the card; the statement settlement is a separate transfer, not a second expense (§15.2, V009).".into();

    let series = vec![
        salary_a,
        series(
            SALARY_B,
            "Person B salary",
            Direction::Income,
            AmountSpec::Exact(pkr(300_000)),
            Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Indefinite },
            PERSON_B_CHECKING,
            EntityRef::Person(b),
            Certainty::Expected,
            "Salary",
        ),
        series(
            FREELANCE,
            "Freelance payment — Client X",
            Direction::Income,
            AmountSpec::Range { low: pkr(250_000), expected: pkr(300_000), high: pkr(320_000) },
            Recurrence::OneTime { on: DateSpec::Range { earliest: d(2026, 10, 5), expected: d(2026, 10, 10), latest: d(2026, 10, 25) } },
            PERSON_A_CURRENT,
            EntityRef::Person(a),
            Certainty::Expected,
            "Freelance",
        ),
        receivable,
        series(
            RENT,
            "Rent",
            Direction::Expense,
            AmountSpec::Range { low: pkr(180_000), expected: pkr(180_000), high: pkr(200_000) },
            Recurrence::Monthly { every_n_months: 1, day: 1, from: d(2026, 10, 1), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            SHARED_SAVINGS,
            EntityRef::Household,
            Certainty::Contractual,
            "Housing",
        ),
        series(
            OTHER_SPENDING,
            "Ordinary household expenses",
            Direction::Expense,
            AmountSpec::Range { low: pkr(120_000), expected: pkr(130_000), high: pkr(140_000) },
            Recurrence::Monthly { every_n_months: 1, day: 15, from: d(2026, 9, 15), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            PERSON_B_CHECKING,
            EntityRef::Household,
            Certainty::UserEstimated,
            "Living",
        ),
        series(
            SCHOOL_FEES,
            "School fees",
            Direction::Expense,
            AmountSpec::Exact(pkr(250_000)),
            Recurrence::Monthly { every_n_months: 3, day: 5, from: d(2026, 12, 5), until: Until::Date(d(2028, 6, 5)), invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            SHARED_SAVINGS,
            EntityRef::Household,
            Certainty::Contractual,
            "Education",
        ),
        car_down_payment,
        series(
            ALPHA_REVENUE,
            "Company Alpha client revenue",
            Direction::Income,
            AmountSpec::Range { low: pkr(1_000_000), expected: pkr(1_200_000), high: pkr(1_400_000) },
            Recurrence::Monthly { every_n_months: 1, day: 20, from: d(2026, 9, 20), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            ALPHA_OPERATING,
            EntityRef::Company(ALPHA),
            Certainty::Expected,
            "Revenue",
        ),
        series(
            ALPHA_PAYROLL_RUN,
            "Company Alpha payroll run (employees #42, #43)",
            Direction::Expense,
            AmountSpec::Exact(pkr(700_000)),
            Recurrence::Monthly { every_n_months: 1, day: 25, from: d(2026, 9, 25), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            ALPHA_OPERATING,
            EntityRef::Company(ALPHA),
            Certainty::Contractual,
            "Payroll",
        ),
        series(
            VISA_SETTLEMENT,
            "Visa statement settlement",
            Direction::Transfer { to: PERSON_A_VISA },
            AmountSpec::Exact(pkr(85_000)),
            Recurrence::OneTime { on: DateSpec::Exact(d(2026, 9, 28)) },
            PERSON_A_CURRENT,
            EntityRef::Person(a),
            Certainty::Contractual,
            "Liability settlement",
        ),
        series(
            CAR_INSURANCE,
            "Car insurance premium",
            Direction::Expense,
            AmountSpec::Exact(pkr(96_000)),
            Recurrence::Yearly { month: 9, day: 5, from: d(2026, 9, 5), until: Until::Indefinite },
            SHARED_SAVINGS,
            EntityRef::Household,
            Certainty::Contractual,
            "Insurance",
        ),
        card_purchases,
        series(
            ATM_WITHDRAWAL,
            "ATM cash withdrawal",
            Direction::Transfer { to: CASH_WALLET },
            AmountSpec::Exact(pkr(80_000)),
            Recurrence::Monthly { every_n_months: 1, day: 10, from: d(2026, 9, 10), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            PERSON_A_CURRENT,
            EntityRef::Person(a),
            Certainty::UserEstimated,
            "Cash withdrawal",
        ),
        series(
            FOREIGN_SUBSCRIPTION,
            "Foreign software subscription (USD, on the card)",
            Direction::Expense,
            AmountSpec::Exact(pkr(12_000)),
            Recurrence::Monthly { every_n_months: 1, day: 3, from: d(2026, 10, 3), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            PERSON_A_VISA,
            EntityRef::Person(a),
            Certainty::Contractual,
            "Foreign card",
        ),
    ];

    // §5.7 / §16 — actual transactions and their reconciliation links.
    let actuals = vec![
        ActualTransaction {
            id: TXN_INSURANCE,
            date: d(2026, 9, 4),
            account: SHARED_SAVINGS,
            amount: pkr(-96_000),
            description: "INSURECO annual premium".into(),
        },
        ActualTransaction {
            id: TXN_RECEIVABLE_PART,
            date: d(2026, 9, 9),
            account: PERSON_A_CURRENT,
            amount: pkr(150_000),
            description: "CLIENT X part payment inv 2026-031".into(),
        },
    ];
    let links = vec![
        ReconciliationLink { series: CAR_INSURANCE, original_due: d(2026, 9, 5), transaction: TXN_INSURANCE, amount: pkr(96_000) },
        ReconciliationLink { series: CLIENT_RECEIVABLE, original_due: d(2026, 11, 10), transaction: TXN_RECEIVABLE_PART, amount: pkr(150_000) },
    ];

    let assumptions = vec![
        Assumption {
            id: AssumptionId::new(1),
            text: "Person A receives salary between 480,000 and 520,000 in Sep, Oct and Nov.".into(),
            certainty: Certainty::Contractual,
            source: AssumptionSource::DerivedFromHistory {
                formula: "minimum and maximum of the last 6 reconciled salary payments".into(),
                sample_size: 6,
                sample_from: d(2026, 3, 31),
                sample_to: d(2026, 8, 31),
            },
            accepted_on: Some(d(2026, 9, 10)),
            expires_on: Some(d(2026, 12, 31)),
            applies_to: vec![SALARY_A],
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(2),
            text: "Person B remains employed through Nov 30.".into(),
            certainty: Certainty::Expected,
            source: AssumptionSource::UserEntered,
            accepted_on: Some(d(2026, 9, 10)),
            expires_on: Some(d(2026, 11, 30)),
            applies_to: vec![SALARY_B],
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(3),
            text: "Monthly rent remains ≤ 200,000 through Dec.".into(),
            certainty: Certainty::Contractual,
            source: AssumptionSource::UserEntered,
            accepted_on: Some(d(2026, 5, 1)),
            expires_on: None,
            applies_to: vec![RENT],
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(4),
            text: "The 350,000 client receivable arrives no later than Nov 15.".into(),
            certainty: Certainty::Expected,
            source: AssumptionSource::UserEntered,
            accepted_on: Some(d(2026, 9, 10)),
            expires_on: Some(d(2026, 11, 15)),
            applies_to: vec![CLIENT_RECEIVABLE],
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(5),
            text: "No unplanned expense greater than the configured 150,000 buffer occurs.".into(),
            certainty: Certainty::UserEstimated,
            source: AssumptionSource::UserEntered,
            accepted_on: None,
            expires_on: None,
            applies_to: Vec::new(),
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(6),
            text: "Company Alpha maintains its payroll reserve and can legally distribute 500,000 by Nov 10.".into(),
            certainty: Certainty::Tentative,
            source: AssumptionSource::UserEntered,
            accepted_on: None,
            expires_on: Some(d(2026, 11, 10)),
            applies_to: Vec::new(),
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(7),
            text: "Applicable withdrawal and distribution tax rules remain unchanged.".into(),
            certainty: Certainty::Expected,
            source: AssumptionSource::RulePack("DEMO-JURISDICTION-2026-v2".into()),
            accepted_on: Some(d(2026, 9, 10)),
            expires_on: Some(d(2027, 6, 30)),
            applies_to: Vec::new(),
            private_to: None,
        },
        Assumption {
            id: AssumptionId::new(8),
            text: "Person A leaves Company Alpha at the end of September 2026; no severance or notice-period pay is modelled.".into(),
            certainty: Certainty::ScenarioOnly,
            source: AssumptionSource::Scenario(LEAVE_JOB),
            accepted_on: None,
            expires_on: None,
            applies_to: vec![SALARY_A],
            private_to: Some(a),
        },
    ];

    let scenarios = vec![
        Scenario {
            id: BUY_CAR,
            name: "Buy car".into(),
            description: "8,000,000 car, purchase window Oct–Mar, down payment 2,000,000–5,000,000, keep the 1,000,000 household reserve (§19).".into(),
            private_to: None,
            changes: Vec::new(),
            composed_of: Vec::new(),
        },
        Scenario {
            id: LEAVE_JOB,
            name: "Leave job".into(),
            description: "Person A resigns from Company Alpha; the last salary is paid on 30 Sep 2026 and a replacement is hired (§18.5, private).".into(),
            private_to: Some(a),
            changes: vec![
                crate::scenario::ScenarioChange::EndSeries { series: SALARY_A, last_on: d(2026, 9, 30), reason: "resignation effective end of September 2026".into() },
                crate::scenario::ScenarioChange::AddEmployment {
                    company: ALPHA,
                    employee: Employee { name: "Replacement hire".into(), person: None, monthly_gross: pkr(400_000), start: d(2026, 10, 1), end: None },
                },
            ],
            composed_of: Vec::new(),
        },
        Scenario {
            id: LEAVE_JOB_AND_CAR,
            name: "Leave job + buy car".into(),
            description: "Composition (§18.3): both overlays applied together; compatible because they touch different series.".into(),
            private_to: Some(a),
            changes: Vec::new(),
            composed_of: vec![LEAVE_JOB, BUY_CAR],
        },
    ];

    // §12 — fictitious DEMO rule packs: the 2026 pack and a 2027 successor whose
    // only change is the withholding rate, so §25 version pinning is visible.
    let demo_rules = |year: i32, withholding_bp: u32| {
        vec![
            TaxRule {
                id: TaxRuleId::new(if year == 2026 { 1 } else { 11 }),
                name: "DEMO cash withdrawal withholding".into(),
                tax_type: "Withholding on cash withdrawal".into(),
                categories: vec!["Cash withdrawal".into()],
                scope: "Personal bank accounts at Bank A and Bank B".into(),
                kind: TaxKind::FlatAboveThreshold { rate_basis_points: 60, threshold: pkr(50_000), basis: ThresholdBasis::PerTransaction, on_excess_only: false },
                timing: TaxTiming::WithheldAtSource { creditable: true },
                effective_from: d(year, 1, 1),
                effective_to: Some(d(year, 12, 31)),
                source: "§14.3 rule-engine example — fictitious, not any country's law".into(),
                explanation: "0.6% on the full withdrawal once a single withdrawal exceeds 50,000; creditable at assessment.".into(),
            },
            TaxRule {
                id: TaxRuleId::new(if year == 2026 { 2 } else { 12 }),
                name: "DEMO foreign card tax".into(),
                tax_type: "Card transaction tax".into(),
                categories: vec!["Foreign card".into()],
                scope: "Credit-card payments in a currency other than the account base currency".into(),
                kind: TaxKind::FlatRate { rate_basis_points: 500 },
                timing: TaxTiming::Immediate,
                effective_from: d(year, 1, 1),
                effective_to: Some(d(year, 12, 31)),
                source: "§14.4 rule-engine example — fictitious".into(),
                explanation: "5% tax event on the card; the bank's 1.5% fee is a separate fee event (M7).".into(),
            },
            TaxRule {
                id: TaxRuleId::new(if year == 2026 { 3 } else { 13 }),
                name: "DEMO salary withholding".into(),
                tax_type: "Income tax withheld at source".into(),
                categories: vec!["Salary".into()],
                scope: "Salaries paid to persons".into(),
                kind: TaxKind::FlatRate { rate_basis_points: withholding_bp },
                timing: TaxTiming::WithheldAtSource { creditable: true },
                effective_from: d(year, 1, 1),
                effective_to: Some(d(year, 12, 31)),
                source: "fictitious".into(),
                explanation: format!("{}% of gross salary withheld by the employer and credited against the annual assessment (M24).", withholding_bp / 100),
            },
            TaxRule {
                id: TaxRuleId::new(if year == 2026 { 4 } else { 14 }),
                name: "DEMO annual income tax".into(),
                tax_type: "Income tax (annual assessment)".into(),
                categories: vec!["Salary".into(), "Freelance".into(), "Receivable".into()],
                scope: "Taxable income of persons within the forecast window".into(),
                kind: TaxKind::AnnualBrackets {
                    brackets: vec![
                        Bracket { lower: pkr(0), upper: Some(pkr(600_000)), rate_basis_points: 0 },
                        Bracket { lower: pkr(600_000), upper: Some(pkr(2_400_000)), rate_basis_points: 1_000 },
                        Bracket { lower: pkr(2_400_000), upper: None, rate_basis_points: 3_000 },
                    ],
                },
                timing: TaxTiming::AnnualAssessment { due_month: 9, due_day: 30 },
                effective_from: d(year, 1, 1),
                effective_to: Some(d(year, 12, 31)),
                source: "fictitious brackets in the shape of M23".into(),
                explanation: "Marginal brackets on the year's taxable income inside the window; creditable withholding is deducted at assessment; the balance is payable 30 Sep of the following year (§12.3).".into(),
            },
        ]
    };
    let tax_packs = vec![
        TaxRulePack {
            name: "DEMO-JURISDICTION-2026-v2".into(),
            version: "v2".into(),
            jurisdiction: "DEMO — fictitious rule-engine examples, not any country's law (§14.3)".into(),
            verified: false,
            rules: demo_rules(2026, 700),
        },
        TaxRulePack {
            name: "DEMO-JURISDICTION-2027-v1".into(),
            version: "v1".into(),
            jurisdiction: "DEMO — fictitious, effective 2027 (§25: a 2027 forecast must not silently use 2026 rules)".into(),
            verified: false,
            rules: demo_rules(2027, 800),
        },
    ];

    let mut private_account_policy = preset_policy(
        2,
        ObjectRef::Account(PERSON_A_CURRENT),
        vec![a],
        VisibilityPreset::Private,
        CalculationAccess::RestrictedContribution,
    );
    // §7.5 example: balance to A + B, transactions A only, usable in household
    // forecasts, disclosed as an aggregate only.
    private_account_policy.forecasts = Grantees::Household;
    private_account_policy.restricted_disclosure = Disclosure::Aggregate;
    private_account_policy.version = 2;
    private_account_policy.previous = Some("fully private (v1)".into());
    private_account_policy.purposes = vec!["household forecasts".into(), "scenario: Buy car".into()];

    let mut alpha_policy = preset_policy(9, ObjectRef::Company(ALPHA), vec![a], VisibilityPreset::SharedSummary, CalculationAccess::Excluded);
    // §8.7: household members know the company exists and see a planning-safe
    // output; balances, payroll and clients stay with the owner.
    alpha_policy.existence = Grantees::Household;
    let mut beta_policy = preset_policy(10, ObjectRef::Company(BETA), vec![a], VisibilityPreset::SharedSummary, CalculationAccess::Excluded);
    beta_policy.existence = Grantees::Household;

    let policies = vec![
        preset_policy(1, ObjectRef::Account(SHARED_SAVINGS), vec![a, b], VisibilityPreset::FullyShared, CalculationAccess::Full),
        private_account_policy,
        preset_policy(3, ObjectRef::Account(PERSON_B_CHECKING), vec![b], VisibilityPreset::FullyShared, CalculationAccess::Full),
        preset_policy(4, ObjectRef::Account(PERSON_A_VISA), vec![a], VisibilityPreset::SharedBalance, CalculationAccess::Full),
        preset_policy(5, ObjectRef::Account(FIXED_DEPOSIT), vec![a, b], VisibilityPreset::FullyShared, CalculationAccess::Full),
        preset_policy(6, ObjectRef::Account(ALPHA_OPERATING), vec![a], VisibilityPreset::SharedSummary, CalculationAccess::Excluded),
        preset_policy(7, ObjectRef::Account(ALPHA_PAYROLL), vec![a], VisibilityPreset::Private, CalculationAccess::Excluded),
        preset_policy(8, ObjectRef::Account(BETA_OPERATING), vec![a], VisibilityPreset::SharedSummary, CalculationAccess::Excluded),
        preset_policy(15, ObjectRef::Account(CASH_WALLET), vec![a], VisibilityPreset::SharedBalance, CalculationAccess::Full),
        alpha_policy,
        beta_policy,
        preset_policy(11, ObjectRef::Scenario(BUY_CAR), vec![a, b], VisibilityPreset::FullyShared, CalculationAccess::Full),
        preset_policy(12, ObjectRef::Scenario(LEAVE_JOB), vec![a], VisibilityPreset::Private, CalculationAccess::Excluded),
        preset_policy(16, ObjectRef::Scenario(LEAVE_JOB_AND_CAR), vec![a], VisibilityPreset::Private, CalculationAccess::Excluded),
        preset_policy(13, ObjectRef::Person(a), vec![a], VisibilityPreset::FullyShared, CalculationAccess::Full),
        preset_policy(14, ObjectRef::Person(b), vec![b], VisibilityPreset::FullyShared, CalculationAccess::Full),
    ];

    Household {
        name: "Plan household (fictitious)".into(),
        base_currency: Currency::PKR,
        as_of: as_of(),
        people,
        companies,
        accounts,
        reservations,
        series,
        assumptions,
        scenarios,
        tax_packs,
        policies,
        actuals,
        links,
        rules: demo_rules_for(a),
        rule_tie_break: crate::rules::TieBreak::OldestRule,
        history: vec![
            HistoricalPayment { series: SALARY_A, date: d(2026, 3, 31), amount: pkr(500_000) },
            HistoricalPayment { series: SALARY_A, date: d(2026, 4, 30), amount: pkr(480_000) },
            HistoricalPayment { series: SALARY_A, date: d(2026, 5, 31), amount: pkr(505_000) },
            HistoricalPayment { series: SALARY_A, date: d(2026, 6, 30), amount: pkr(520_000) },
            HistoricalPayment { series: SALARY_A, date: d(2026, 7, 31), amount: pkr(495_000) },
            HistoricalPayment { series: SALARY_A, date: d(2026, 8, 31), amount: pkr(500_000) },
            HistoricalPayment { series: RENT, date: d(2026, 6, 1), amount: pkr(180_000) },
            HistoricalPayment { series: RENT, date: d(2026, 7, 1), amount: pkr(180_000) },
            HistoricalPayment { series: RENT, date: d(2026, 8, 1), amount: pkr(180_000) },
            HistoricalPayment { series: RENT, date: d(2026, 9, 1), amount: pkr(180_000) },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_internally_consistent() {
        let household = plan_household();
        for account in &household.accounts {
            assert!(household.policy_for(ObjectRef::Account(account.id)).is_some(), "{} needs a policy", account.name);
            if let Holder::Persons(shares) = &account.holder {
                assert_eq!(shares.iter().map(|s| s.basis_points).sum::<u32>(), 10_000, "{} shares must total 100%", account.name);
            }
        }
        for reservation in &household.reservations {
            assert!(household.account(reservation.account).is_some());
        }
        for link in &household.links {
            assert!(household.actual(link.transaction).is_some());
            assert!(household.series_by_id(link.series).is_some());
        }
        for series in &household.series {
            assert!(household.account(series.account).is_some(), "{}", series.name);
            if let Some(scenario) = series.scenario {
                assert!(household.scenario(scenario).is_some());
            }
        }
        let ids: Vec<_> = household.policies.iter().map(|p| p.id).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "policy ids are unique");
    }
}
