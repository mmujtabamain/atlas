# Household Financial Planning & Decision Engine
## Research-expanded requirements analysis · Version 3.2

**Status:** Full requirements discovery; delivery scoping intentionally unspecified  
**Research updated:** 2026-09-10  
**Supersedes:** The earlier requirements document where definitions, examples or scope differ.  
**Evidence:** Primary research, author-hosted manuscripts, official standards and official technical documentation.  
**Illustrations:** All unlabeled monetary amounts, tax rates, dates and strategy outcomes are hypothetical, not actual user finances or jurisdictional advice.  
**Working description:** A deterministic, user-controlled financial timeline and decision engine for households, people, accounts, companies, taxes, future obligations, and what-if scenarios.

---

## Reading guide and revision scope

This document retains the earlier household, company, tax, rule-engine and scenario requirements, corrects their numerical/semantic weaknesses, and adds a research-backed mathematical specification and a broad feature inventory. It does not select an implementation stack, prescribe a UX, or divide the product into delivery stages, milestones, releases, or implementation priorities.

| Part | Contents |
|---|---|
| Sections 1–21 | Integrated product/domain requirements, with corrected cash, uncertainty and tax semantics |
| Sections 22–30 | Deferred decisions, scope policy, provenance, data model and recommendation contract |
| [31. Research method](#research-method) | What the research does and does not establish |
| [32. Mathematical contract](#mathematical-contract) | Units, state, assumptions, methods and guarantee terminology |
| [33–42. Model catalogue](#model-catalogue) | Equations, proposed uses, limits and traceable research foundations |
| [43. Feature register](#feature-register) | Individually identified candidate requirements, without delivery scoping |
| [44. Worked examples](#worked-examples) | Reproducible cases for liquidity, taxes, robust affordability and risk |
| [45. Verification requirements](#verification) | Numerical, accounting, solver and scenario acceptance tests |
| [46. Research and policy decisions](#open-decisions) | Questions that must be resolved before implementing applicable modules |
| [47. References](#references) | Linked primary sources and evidence-access notes |

**What changed materially:** future certainty is now an explicit mathematical contract; joint uncertainty replaces reliance on three hand-picked cases; multi-period funding replaces purely greedy withdrawal order; settlement-aware cash constraints replace aggregated affordability; the research inventory includes advanced risk, investment, business, insurance and lifetime-planning candidates; and **privacy is now a foundational authorization model rather than a postponed concern**. Every financial object can carry separate economic-ownership, visibility, calculation-access and disclosure semantics.

**What did not change:** no AI; the user controls assumptions and objectives; all calculation and recommendation logic is deterministic and inspectable. Companies remain legally distinct from people. UX design remains deferred.

**Privacy architecture changed materially:** authorization is now a foundational data-model requirement. Economic ownership, legal/entity boundaries, raw-data visibility, permission to participate in calculations, and permission to view derived explanations are separate concepts. Detailed privacy UX may be designed later, but the underlying authorization model must exist from the beginning.

**A necessary distinction:** an accounting identity can be exact. An optimizer can produce a model-feasibility or optimality certificate subject to its numerical conditions. No paper makes future salaries, tax laws, investment returns or payment dates certain. Any guarantee must name the model, uncertainty set, time horizon, rule versions and assumptions to which it applies.

---

## 1. Executive Summary

This product should **not** be designed primarily as an expense tracker or conventional budgeting application. Expense tracking is one subsystem inside a broader product.

The core problem is:

> Given everything the household and its members own today, the accounts and companies through which money moves, all known or assumed future inflows and outflows, applicable taxes and rules, and the decisions being considered, what could the financial position be on any future date, and under exactly which assumptions?

The application should function as a **household treasury, forecasting, and financial decision engine**.

It must support:

- Multiple people in one household or planning group.
- Multiple bank and financial accounts per person.
- Joint/shared accounts.
- Multiple companies owned or operated by household members.
- Company accounts, payroll, salaries, owner compensation and business cash obligations.
- One-time, recurring, scheduled, ending, changing, and uncertain income sources.
- One-time, recurring, scheduled, ending, changing, and uncertain expenses.
- Transfers that do not incorrectly become income or expenses.
- Current, future, and scenario-specific tax calculations.
- Deterministic tax-minimization analysis for withdrawals, distributions and funding choices.
- User-defined financial, tax, account-selection and funding rules.
- Future balance forecasting that never treats uncertain future income as money already available today.
- Explicit assumptions for every future-looking result.
- Major-purchase analysis such as cars, homes, education, weddings, travel, business funding, debt repayment, etc.
- Alternative scenarios and comparison of their consequences.
- Full explainability for every calculated value and recommendation, subject to viewer authorization.
- Privacy-preserving household planning in which restricted financial data may participate in an authorized calculation without necessarily exposing the underlying account, balance, transaction history, assumptions, or scenario to other participants.
- No AI or machine-learning calculation, classification, forecasting or recommendation logic in the product defined here.

The product principle is:

> **The application performs deterministic calculations. The user supplies, accepts, rejects, or edits the assumptions.**

A second principle is equally important:

> **Future money is not current money. A forecast may depend on expected future money, but it must never silently promote that money into currently available funds.**

---

## 2. Non-Negotiable Product Principles

### 2.1 Every derived value must have complete provenance; disclosure is authorization-aware

This is a hard requirement, not an optional debug feature.

Any derived figure must have a complete inspectable calculation graph. A viewer receives the portion of that graph permitted by authorization. Examples include:

- Current household liquidity.
- Available discretionary cash.
- Projected balance on a future date.
- Estimated tax due.
- Effective or marginal tax rate.
- Lowest projected balance.
- Recommended withdrawal sequence.
- Suggested month for a major purchase.
- Suggested down payment range.
- Cash runway.
- Goal feasibility.
- Company distributable cash.
- Household amount available after extracting money from a company.

An authorized viewer must be able to answer:

> **Why is this number this number?**

The complete calculation graph must exist internally. What each viewer may inspect is governed by authorization. Restricted provenance must never be exposed merely because a private value contributed to an authorized aggregate.

For a projected balance, for example, the application should be able to provide the exact calculation chain:

```text
Current reconciled liquid cash                     1,400,000
+ contractually expected future salary               600,000
+ other assumed future salary                        300,000
+ expected future freelance payment                  250,000
- future rent                                        240,000
- future ordinary expenses                           320,000
- planned car down payment                           400,000
------------------------------------------------------------
Conditional projected cash                         1,590,000
- non-overlapping tax earmark                         120,000
------------------------------------------------------------
Conditional projected unreserved cash               1,470,000
```

Only the starting cash is already received. Contractually expected receipts are still conditional. A tax earmark reduces unreserved cash, not the bank balance; an actual tax payment later reduces cash and releases the matching earmark. Calculation provenance must preserve these distinctions.

### 2.2 All calculations and recommendations must be deterministic and inspectable

> **All financial calculations, classifications, projections, scenario comparisons and recommendations must be produced by deterministic, inspectable logic.**

This applies to the entire core product.

The application must not require an LLM or machine-learning model to:

- Calculate balances.
- Categorize a transaction when a deterministic user rule exists.
- Forecast scheduled income.
- Calculate tax.
- Select an account from which to fund an expense.
- Compare withdrawal strategies.
- Search purchase dates/down payments.
- Calculate affordability.
- Compare scenarios.
- Explain why one scenario produces a different result from another.

Deterministic algorithms, formulas, rule engines, recurrence engines, constraint solvers and exhaustive/heuristic parameter search are allowed.

### 2.3 No opaque confidence

The application should not say:

> “We are 92% confident you can afford the car.”

unless the user has explicitly supplied a probabilistic model that mathematically supports that probability.

The preferred form is:

> “You could purchase the car in November while maintaining your 1,000,000 minimum reserve **under the following assumptions**.”

The assumptions then remain visible and inspectable.

### 2.4 Future money is conditional, not available

Expected income must not increase today's spendable balance.

The system therefore distinguishes at least:

1. **Confirmed current money** — already present and reconciled.
2. **Reserved current money** — present, but intentionally unavailable for general spending.
3. **Free current money** — present and not reserved.
4. **Expected future money** — not yet received and always assumption-bound.
5. **Conditional future money** — depends on an event, scenario, rule or uncertain assumption.

A future projection can include expected income, but the projection must retain the fact that it is expected rather than confirmed.

### 2.5 User-controlled assumptions

The application should not quietly invent assumptions.

Assumptions can be:

- Explicitly entered by the user.
- Derived deterministically from historical data using an inspectable formula.
- Imported from a rule pack.
- Created by a scenario.

If the application derives an assumption, it must show how.

Example:

> Salary assumption: 480,000–520,000 per month.  
> Derived from the minimum and maximum of the last 6 reconciled salary payments.  
> User accepted on 2026-09-10.


### 2.6 Authorization-aware explainability is non-negotiable

The original explainability requirement is refined as follows:

> **Every derived value must expose its complete inputs and calculation to an authorized viewer. Other viewers receive the most detailed privacy-preserving explanation they are authorized to receive, while the displayed result remains mathematically consistent with the underlying calculation.**

Authorization must be evaluated independently from economic ownership. A financial object may be privately owned, jointly owned, fully visible, partially visible, hidden but permitted to contribute to a calculation, or excluded from a calculation.

The engine must never solve the explainability/privacy conflict by either:

- revealing restricted source data in provenance; or
- returning an unexplained opaque number when a privacy-preserving explanation can be generated.

A restricted contribution may therefore be represented as an authorized aggregate, for example:

```text
Current household-visible cash                       1,400,000
+ owner-authorized restricted contribution             500,000
  underlying account details not disclosed
- planned household obligations                        700,000
--------------------------------------------------------------
Projected household-usable cash                      1,200,000
```

The account owner, if authorized, can inspect the complete source chain. A different household member may only see the permitted aggregate. Privacy filtering applies to calculations, explanations, scenario comparisons, exports, notifications, logs and historical replay.

---

## 3. Product Category and Scope

The product is better described as a:

- Personal/household treasury manager.
- Financial timeline.
- Cash-flow forecasting engine.
- Scenario simulator.
- Affordability/decision engine.
- Tax-aware funding optimizer.

It is **not primarily**:

- A receipt scanner.
- A simple expense category tracker.
- An envelope budget.
- A chatbot.
- An accounting package for statutory company books.
- A tax filing product.

It may integrate or expand into some of those areas later, but they should not define the requirements architecture.

---

## 4. Existing Products and Requirements Worth Borrowing

The objective of this research is not to decide whether an existing product should replace this application. Existing products are reference implementations from which useful requirements can be extracted.

### 4.1 PocketSmith

Carried-forward product reference: [P01](#p01).

Useful patterns:

- Calendar-based future planning.
- One-off and recurring future events.
- Account-level balance forecasting.
- Scenario/what-if modelling.
- Future projections that begin from actual account balances.
- Modifying a specific occurrence versus future occurrences in a series.

Requirement to borrow:

> Treat financial plans as a chronological timeline rather than only as monthly budgets.

### 4.2 ProjectionLab

Carried-forward product reference: [P02](#p02), [P03](#p03).

Useful patterns:

- Income streams with explicit start and end dates.
- Life events and dated changes that alter future financial conditions.
- Advanced tax analytics.
- Contribution/drawdown sequencing.
- Multiple-account funding rules.
- Scenario optimization using explicit targets and constraints.

Requirement to borrow:

> Jobs, income streams, assets, expenses and tax strategies should be time-bounded and scenario-aware rather than assumed to continue forever.

ProjectionLab also demonstrates the usefulness of jurisdiction-specific tax presets and deterministic optimization of tax strategies.

### 4.3 Monarch Money

Carried-forward product reference: [P08](#p08).

Useful patterns:

- Multiple household users.
- Ownership concepts for accounts and transactions.
- Shared household financial context.

Requirement to borrow:

> Ownership should be represented explicitly rather than inferred from account names or categories.

The product must go further than a household-wide shared view: **economic ownership, visibility, and permission to participate in calculations must be modeled independently**. Advanced privacy UX can be deferred, but the underlying authorization boundary cannot be.

### 4.4 Quicken Simplifi

Carried-forward product reference: [P10](#p10).

Useful patterns:

- Recurring bills and income.
- Upcoming cash activity.
- Projected account cash flow.

Requirement to borrow:

> Planned future transactions should visibly affect future account balances before they occur, while remaining distinguishable from actual transactions.

### 4.5 Actual Budget

Carried-forward product reference: [P07](#p07).

Useful patterns:

- Flexible recurring schedule definitions.
- One-time schedules.
- Indefinite schedules.
- Schedules with an end date.
- Non-trivial recurrence rules.

Requirement to borrow:

> The recurrence system should be closer to a calendar recurrence engine than a simple “monthly recurring” toggle.

### 4.6 YNAB

Carried-forward product reference: [P09](#p09).

The useful principle here is **not** that future money should never be forecast. The useful principle is that expected future income should not become money the user is considered to possess today.

This application should preserve that safety principle while adding a much stronger future-planning layer.

Therefore:

- Current available cash uses only current confirmed funds.
- Future projections may include expected funds.
- Any recommendation that depends on those funds must explicitly list the assumptions.

### 4.7 RightCapital

Carried-forward product reference: [P04](#p04), [P05](#p05).

Useful tax-planning patterns:

- Explicit withdrawal sequencing between account/tax buckets.
- Side-by-side strategy comparison.
- Tax strategy details that show resulting account balances, withdrawals and taxes.
- Deterministically testing combinations of tax strategies.

Requirement to borrow:

> Tax-efficient funding should be modeled as a constrained scenario comparison, not as an unexplained recommendation.

### 4.8 Holistiplan

Carried-forward product reference: [P06](#p06).

Useful patterns:

- Baseline versus modified tax scenarios.
- Incremental tax-cost calculation.
- “Solve for max” style analysis for tax thresholds.
- Comparison of tax consequences when an additional transaction or conversion is introduced.

Requirement to borrow:

> For any proposed extraction or taxable action, calculate taxes both with and without the action so the user can inspect the incremental tax consequence.

---

## 5. Core Domain Model

The central model should be a **financial timeline**, not a transaction table.

The fundamental concepts are:

### 5.1 Household

A planning boundary containing people, shared resources, assumptions and scenarios.

### 5.2 Person

A human participant in the household.

A person may:

- Own accounts.
- Co-own accounts.
- Earn salaries.
- Have freelance/business income.
- Own one or more companies.
- Receive compensation from their companies.
- Owe taxes.
- Own liabilities.
- Contribute to shared household expenses.

### 5.3 Company

A separate economic entity associated with one or more persons.

A company must **not** be treated as merely another personal bank account.

It may contain:

- Business bank accounts.
- Business credit cards.
- Employees.
- Payroll obligations.
- Revenue streams.
- Operating expenses.
- Tax liabilities.
- Loans.
- Owner contributions.
- Owner compensation/distributions.
- Retained working capital.

### 5.4 Account

A container for money, investments, debt, receivables or another financial balance.

Examples:

- Checking/current account.
- Savings account.
- Cash wallet.
- Credit card.
- Loan.
- Brokerage account.
- Fixed deposit.
- Company account.
- Tax reserve account.

### 5.5 Financial Event

Anything that changes financial state at a point in time.

Examples:

- Salary payment.
- Rent.
- Client invoice payment.
- Payroll run.
- Tax payment.
- Transfer.
- Loan repayment.
- Car down payment.
- Dividend/distribution.
- Owner reimbursement.

### 5.6 Event Series

A template that generates event occurrences over time.

Examples:

- Salary every month until December.
- Rent every month indefinitely.
- Quarterly school fees until graduation.
- Employee payroll every month while employment is active.

### 5.7 Actual Transaction

A financial movement that actually occurred.

Actual transactions should be reconcilable to planned event occurrences.

### 5.8 Reservation

A claim on existing money without requiring the money to be stored in a separate bank account.

Examples:

- Emergency fund.
- Tax reserve.
- Tuition reserve.
- Car reserve.

### 5.9 Assumption

A condition that a forecast or recommendation depends upon.

### 5.10 Rule

A deterministic instruction affecting classification, calculation, funding, tax, recurrence, account selection or scenario evaluation.

### 5.11 Scenario

An alternative set of future events, assumptions and rules applied over a baseline.

### 5.12 Goal / Decision

A financial objective or proposed action to test.

Examples:

- Buy a car.
- Move home.
- Leave a job.
- Start a business.
- Pay off debt.
- Fund university.

### 5.13 AccessPolicy

A versioned authorization policy attached to a financial object or inherited from an authorized boundary. It defines which viewers may discover, inspect, calculate with, explain, export or modify the object.

### 5.14 AccessGrant

A scoped grant from an owner or authorized administrator to a person, role or planning context. A grant may be permanent, effective-dated, temporary, scenario-specific or purpose-specific.

### 5.15 HouseholdRole

A household-level authorization role such as owner, household member, dependent, adviser or read-only participant. Roles are conveniences for grants, not substitutes for object-level policy.

### 5.16 EntityRole

A role within a company or other legal entity, such as owner/director, finance administrator, payroll operator, employee or read-only adviser. Entity roles must not automatically confer household access.

### 5.17 DisclosurePolicy

A rule defining how restricted data may be represented to a viewer: hidden, aggregate only, balance only, selected fields, or full details.

### 5.18 PrivacyAuditEvent

An immutable record of material access-policy changes and authorization-sensitive actions, including who changed a policy, what changed, its effective time and the policy version used by a historical calculation.

---

## 6. Money and Balance Definitions

The application must avoid one ambiguous “balance” number.

At minimum, it should distinguish:

### 6.1 Total assets

Everything included in the planning boundary that has positive economic value.

### 6.2 Liabilities

Everything owed.

### 6.3 Net worth

Assets minus liabilities.

### 6.4 Liquid cash

Money that is currently accessible subject to account constraints.

### 6.5 Reserved cash

Liquid cash earmarked for another obligation or rule.

### 6.6 Free current cash

Current settled, accessible cash remaining after non-overlapping earmarks and applicable funding constraints. Nested minimums are constraints, not automatically additive deductions. Preserve negative signed headroom as a warning; a displayed spendable amount may be floored at zero only when the deficit is separately reported. For multiple accounts/entities, compute feasible accessible funding rather than assuming every balance is fungible. See Models M02–M04.

### 6.7 Projected cash

A future balance generated from current confirmed balances plus future events.

Projected cash must carry its assumption set.

### 6.8 Business cash

Cash held by a company.

This must not automatically be included in personal/household free cash.

### 6.9 Extractable business cash

The amount that could potentially be transferred from a company to a person under a specified extraction method, tax rules, timing constraints, payroll/working-capital rules and scenario.

---

## 7. People, Accounts and Ownership

An account should have properties such as:

- Account name.
- Institution.
- Account type.
- Economic owner(s).
- Ownership percentage(s).
- Household inclusion status.
- Currency.
- Liquidity classification.
- Minimum desired/required balance.
- Transfer delay.
- Fees.
- Tax treatment.
- Source of truth: manual/imported/synchronized.
- Last reconciliation date.
- Whether withdrawals are permitted.
- Whether the account can fund a particular category of expense.

Joint ownership must avoid double counting.

Example:

- Person A owns 50%.
- Person B owns 50%.
- Household aggregation counts the account once at 100%.
- Individual views can attribute the appropriate economic share.

### 7.1 Ownership, visibility and calculation access are separate

Privacy is a foundational data-model requirement even though detailed privacy UX is deferred. Every relevant financial object must have an authorization boundary distinct from economic ownership.

Conceptually:

```text
Financial Object
├── economic_owner / ownership shares
├── legal or entity boundary
├── visibility policy
│   ├── existence
│   ├── balance/value
│   ├── transactions/events
│   ├── metadata
│   ├── forecasts
│   ├── assumptions
│   └── explanations/provenance
├── calculation_access
│   ├── excluded
│   ├── may contribute under restricted disclosure
│   └── fully available to the calculation context
└── permitted viewers / roles / contexts
```

A person's ownership share must remain economically correct regardless of who can see the account. Privacy must never change accounting truth or create double counting.

### 7.2 Minimum visibility policies

The authorization model must be capable of representing at least:

- **Private** — only authorized owners can discover or inspect the object.
- **Private but usable in authorized calculations** — underlying details remain restricted, but a calculation may use a permitted contribution.
- **Summary shared** — a permitted aggregate may be shown without institution, account or transaction details.
- **Balance shared, transactions private** — liquidity/value can be visible while spending history remains restricted.
- **Fully shared** — permitted viewers can inspect the complete object and its provenance.
- **Selected viewers only** — access can be granted to specified people or roles.

Implementation may expose simpler presets initially, such as `Private`, `Shared summary`, `Shared balance`, and `Fully shared`, plus an independent `Include in authorized household calculations: Yes/No`. The data model must not be limited to those presets.

### 7.3 Calculation-access policy

Visibility and calculation use must be independent. Examples:

- A private savings account may be excluded entirely from household affordability.
- It may participate only as an owner-authorized contribution.
- It may contribute to a household liquidity total while its account identity remains hidden.
- It may be authorized only for a named scenario such as `Buy Home`.
- It may be visible to the owner but unavailable to another person's scenario or funding search.

The calculation engine must evaluate authorization before selecting an object as an input, funding source, constraint, assumption or explanation component.

### 7.4 Purpose-specific disclosure

A user should be able to authorize restricted resources for a defined planning purpose without globally sharing them. Examples:

```text
Allow Account 17 to contribute to household affordability calculations
without revealing the balance or transaction history.
```

```text
Allow Account 17 to be considered only in Scenario: Buy Home.
```

Purpose scope should support household forecasts, specific scenarios, specific goals/decisions, tax calculations, funding searches, company-to-person extraction analysis and other explicitly modeled contexts.

### 7.5 Effective-dated and auditable policies

Access policies and grants must be effective-dated, versioned and auditable, consistent with tax rules, assumptions and other deterministic inputs. Example:

```text
Policy owner: Person A
Resource: Account 17
Existence: Person A only
Balance: Person A + Person B
Transactions: Person A only
Use in household forecasts: yes
Disclosure of restricted contribution: aggregate only
Effective from: 2026-09-01
Changed by: Person A
Changed at: 2026-09-10 18:03
Previous policy: fully private
```

Historical calculations must retain the authorization-policy version that governed them. Later permission changes must not silently rewrite what a historical viewer was authorized to see at that time.

### 7.6 Privacy-aware aggregation and inference controls

A restricted object may contribute to an authorized aggregate without exposing its source. However, the engine must also prevent obvious reconstruction through provenance, exports, difference views or repeated scenario queries. For example, if showing a household total before and after toggling a single private account would trivially reveal that account's balance, the disclosure policy may require coarser aggregation, suppression of a component, or an explicit owner-approved disclosure.

This requirement is application-level authorization and disclosure control; it does not by itself claim cryptographic secure multi-party computation or differential privacy. Those can be evaluated as optional future techniques if the threat model later requires them.

---

## 8. Company and Business Finance Requirements

### 8.1 Multiple companies per person

A person may own or manage zero, one or many companies.

Example:

```text
Person A
├── Company Alpha
│   ├── Operating account
│   ├── Payroll account
│   └── Corporate card
└── Company Beta
    ├── Operating account
    └── Savings account
```

### 8.2 Separate company ledger

Company money and household money must remain distinct.

The company ledger should track:

- Current company cash.
- Incoming client/customer payments.
- Payroll.
- Employee salaries.
- Contractors.
- Taxes.
- Rent.
- Loans.
- Recurring operating expenses.
- Accounts payable/receivable when enabled.
- Owner compensation.
- Owner contributions.
- Company reserves.
- Minimum working capital.

### 8.3 Employees and payroll

A company must support multiple employees with:

- Employee/person identifier.
- Salary amount.
- Salary frequency.
- Start date.
- End date.
- Raises or changes with effective dates.
- Bonuses.
- Employer taxes/contributions.
- Employee withholding when relevant.
- One-time payroll adjustments.

Payroll must affect company forecasts.

### 8.4 Linked company-to-person events

If Person A receives a salary from Company Alpha:

- It is an expense/outflow for Company Alpha.
- It is an income/inflow for Person A.
- It should be one linked economic movement, not two unrelated events.

The same pattern applies to:

- Dividends/distributions.
- Owner draws where legally applicable.
- Expense reimbursements.
- Loan repayments to/from owners.
- Capital contributions.

### 8.5 No automatic household access to business cash

A company having 10,000,000 in its bank account does not mean the household has 10,000,000 available to spend.

The system should instead answer:

> If the household needs 2,000,000 from Company Alpha, which lawful extraction methods are available, what taxes/fees and business consequences apply, and what amount reaches the person/household?

### 8.6 Company-level constraints

Examples:

- Maintain at least 3 months of payroll.
- Maintain tax reserve.
- Do not reduce working capital below 2,000,000.
- Do not distribute funds before a specified date.
- Only use dividends when retained earnings or other legal conditions permit, if that jurisdictional rule is represented.
- Salary cannot exceed a user-defined value without explicit override.

The engine should treat these as constraints, not suggestions.


### 8.7 Company authorization and disclosure boundaries

Companies require independent access control because business information may be materially more sensitive than household information.

A household participant may be authorized to see only a planning-safe output such as:

```text
Company Alpha
Potential owner-authorized household extraction this year: 800,000
```

without being permitted to inspect:

- Bank balances.
- Client revenue.
- Customer identities.
- Employee salaries.
- Payroll details.
- Supplier transactions.
- Tax records.
- Individual company-account movements.

Company ownership does not automatically grant household visibility, and household membership does not automatically grant company visibility.

For linked company-to-person movements, authorization is evaluated independently on each side of the link. The same economic movement may therefore have different permitted representations for different viewers while remaining one linked transaction for accounting/reconciliation purposes.

Example:

```text
Company payroll viewer: Salary expense — Employee #42 — 500,000
Employee/recipient:       Salary receipt — Company Alpha — 500,000
Household viewer:         Person A authorized household contribution — 250,000
```

The system must not reveal restricted company data merely because the resulting personal cash flow participates in a household forecast.

---

## 9. Financial Event and Scheduling Engine

Every expected financial movement should support:

- Direction: income / expense / transfer.
- Amount.
- Amount type: exact / range / formula.
- Date.
- Date type: exact / range / rule-derived.
- Recurrence.
- Start date.
- End date.
- Exceptions.
- Linked account(s).
- Person/company owner.
- Category.
- Status.
- Assumption/certainty classification.
- Scenario membership.
- Tax treatment.
- Funding rule.

### 9.1 Recurrence patterns

Support at least:

- One-time.
- Daily.
- Weekly.
- Every N weeks.
- Monthly.
- Every N months.
- Specific days of month.
- Quarterly.
- Annually.
- Custom recurrence.
- Until a date.
- For N occurrences.
- Indefinitely.

### 9.2 Editing a recurrence

A recurring event should support:

- Edit only this occurrence.
- Edit this and future occurrences.
- Edit entire series.
- Skip occurrence.
- Move occurrence.
- Change amount for one occurrence.

### 9.3 Effective-dated changes

Example:

```text
Salary
Sep–Dec: 500,000/month
Jan onward: 550,000/month
```

This must not require deleting and manually recreating every future event.

### 9.4 Employment transitions

The model should cleanly represent:

- Resignation date.
- Last salary date.
- Final payroll adjustment.
- Leave payout.
- Severance.
- Temporary unpaid leave.
- New job start date.
- First new salary date.

---

## 10. Uncertainty and Assumption-Bound Forecasting

This is a core requirement.

### 10.1 No unconditional future claim

The application should avoid:

> “You can afford the car in November.”

Prefer:

> “You could purchase the car in November while retaining the configured reserve, assuming the listed conditions hold.”

### 10.2 Assumption list

Every future-facing recommendation or projection must be able to expose its assumptions.

Example:

```text
Car purchase appears comfortable in November assuming:

1. Person A receives salary between 480,000 and 520,000 in Sep, Oct and Nov.
2. Person B remains employed through Nov 30.
3. Monthly rent remains <= 180,000 through Dec.
4. The 350,000 client receivable arrives no later than Nov 15.
5. No unplanned expense greater than the configured 150,000 buffer occurs.
6. Company Alpha maintains payroll reserve and can legally distribute 500,000 by Nov 10.
7. Applicable withdrawal and distribution tax rules remain unchanged.
```

### 10.3 Types of assumptions

Examples:

- Confirmed.
- Contractual.
- Expected.
- User-estimated.
- Historically derived.
- Scenario-only.
- Tentative.

These labels should not imply statistical probabilities unless probabilities are explicitly modelled.

### 10.4 Amount ranges

Example:

```text
Freelance payment
Expected amount: 300,000
Allowed range: 250,000–320,000
```

### 10.5 Date ranges

Example:

```text
Expected date: Oct 10
Allowed range: Oct 5–Oct 25
```

### 10.6 Named cases are not guaranteed uncertainty envelopes

Without AI, the application can calculate explicit user-defined cases:

- Conservative.
- Expected.
- Optimistic.

For example:

- Conservative uses low income, late receipts and high expenses.
- Expected uses user-selected expected values.
- Optimistic uses high income, early receipts and low expenses.

These are scenario calculations, not confidence claims and not a proof that the true worst case has been checked. Taxes, date interactions and conditional events can make the worst case occur away from an obvious low-income/high-expense corner. The application must also support coherent joint scenario paths and formally specified uncertainty sets, including correlated shocks. A robust-feasibility label requires checking the complete stated set with a valid method; see Models M07–M14.

### 10.7 Historical deterministic assumptions

A user may define a rule such as:

> For future salary planning, assume salary remains between the minimum and maximum of the previous N reconciled salary payments.

The application may calculate that range automatically because the formula is deterministic and inspectable.

Other allowed examples:

- Median of previous N payments.
- Average of previous N payments.
- Fixed amount selected by user.
- Contractual salary amount.
- User-entered range.

The exact method, sample size, data dates, exclusions and user acceptance must be recorded. A historical minimum/maximum is a descriptive sample range, not a bound on the next salary or proof that employment continues.

### 10.8 Assumption sensitivity

The engine must support deterministic sensitivity and reverse-stress analysis:

> Which assumptions would have to fail for this plan to become unsafe?

Example:

```text
Illustrative single-assumption breakpoints, holding every other input fixed:
- Client receipt later than Dec 4 causes a reserve breach.
- Combined Sep–Nov salary below 1,370,000 causes a reserve breach.
- Rent above 230,000/month causes a reserve breach.
- Additional unplanned spending above 260,000 causes a reserve breach.

These separate limits do NOT guarantee safety when several assumptions change
at once. Run joint stress analysis for combinations and shared causes.
```

Both one-at-a-time sensitivity and joint reverse-stress analysis belong in the requirements inventory; neither requires AI.

---

## 11. Forecasting Engine

### 11.1 Deterministic chronological calculation

For any requested future date:

```text
Account cash at event time t
= reconciled starting cash
+ sum of all signed, settled cash postings through t

Taxes, fees, transfers and purchases are included exactly once as postings.
Reservations constrain spendability but are not cash postings.
Future postings belong to a named assumption path, not current available cash.
```

Events must be processed by contractual event time, posting time, settlement time and availability time as applicable. Same-day ordering must be explicit. Native-currency double-entry and a state transition model are specified in Models M01–M06.

### 11.2 Forecast boundaries

Forecasts should be calculable for:

- Individual account.
- Person.
- Company.
- Shared household pool.
- Household liquid position.
- Household net worth.
- Scenario.

### 11.3 Lowest balance matters

Month-end balance alone is insufficient.

The engine should calculate:

- Lowest projected balance.
- Date/time of lowest balance where meaningful.
- Accounts that become negative.
- Reserve breaches.
- Required transfer points.

A household can have sufficient aggregate money while the account used for an automatic payment has insufficient money.

### 11.4 Forecast provenance

Every forecast should carry internally:

- Starting balance snapshot.
- Included event set.
- Included/excluded accounts.
- Applied rules.
- Applied tax rules.
- Applied assumptions.
- Scenario overrides.
- Authorization-policy versions and purpose scope.
- Calculation date/time.

This makes forecasts reproducible. The stored calculation graph must be complete, but **rendered provenance is viewer-authorized**. A viewer may receive full source-level provenance, aggregate provenance, redacted provenance, or a statement that a restricted authorized contribution was included. The numerical result shown to a viewer must be consistent with the disclosure policy governing that result.

---

## 12. Tax Calculation Engine

Taxes must be first-class financial events and liabilities.

### 12.1 Tax rules are effective-dated

Tax rules change over time.

Every tax rule must therefore support:

- Jurisdiction.
- Tax type.
- Effective start date.
- Effective end date or version.
- Thresholds/brackets.
- Rate/formula.
- Exemptions.
- Credits/deductions when modelled.
- Account/person/company scope.
- Source/reference metadata when using a built-in rule pack.

A forecast for 2027 must not silently apply a 2026 rule if the configured rule pack contains a changed 2027 rule.

### 12.2 User-defined tax rules

Because jurisdictions and circumstances vary, users must be able to create/override rules.

Examples:

- X% tax when withdrawing cash from Account A above threshold T.
- Different withdrawal tax for filer/non-filer status.
- Tax on certain credit-card transactions.
- Foreign transaction tax.
- Withholding tax on a bank transaction.
- Dividend tax.
- Salary income tax.
- Capital-gain tax.
- Company distribution tax.
- Payroll withholding.
- Corporate tax estimate.
- Tax on interest/profit income.
- Tax exemption for a defined transaction/account.

### 12.3 Tax liability as an event

A taxable transaction may create:

- Tax paid immediately.
- Withholding at transaction time.
- Current tax accrued but payable later.
- A separate accounting deferred-tax item only where temporary-difference accounting is deliberately implemented; this is not synonymous with a delayed tax payment.
- Installment/advance tax obligation.

The timeline must represent the correct cash date, not only the economic tax amount.

### 12.4 Tax reserve

The system should be able to reserve cash for taxes that have been incurred economically but are payable later.

### 12.5 Incremental tax calculation

For any proposed taxable action:

```text
Incremental tax cost
= tax under scenario with proposed action
- tax under otherwise-identical baseline
```

This makes tax consequences inspectable.

### 12.6 Personal and company tax separation

Tax liabilities should be attributable to the correct legal/economic entity:

- Person A tax.
- Person B tax.
- Company Alpha tax.
- Company Beta tax.

Consolidated household analysis may summarize them, but the liability must not lose its entity attribution.

### 12.7 Tax-calculation caveat

The application may provide tax planning calculations, but a production implementation should clearly distinguish:

- Built-in verified jurisdiction rules.
- User-authored rules.
- Estimated tax.
- Final/legal tax filing amounts.

The application is a planning/calculation system unless a separately validated filing/reporting module is implemented. It must not silently claim to replace statutory tax filing or professional tax advice. No jurisdiction is inferred from language, currency or timezone. See Models M23–M29 and the source requirements in [R20](#r20).

---

## 13. Tax-Minimization and Funding Optimization

The application should support lawful tax minimization under configured rules.

The problem is not simply:

> “Which account has enough money?”

It is:

> “Given the amount required, available accounts/entities, applicable taxes/fees, timing, reserves and user constraints, which valid extraction/funding strategy produces the preferred outcome?”

### 13.1 Example

The household needs 1,000,000.

Available sources:

```text
Personal Account A   1,500,000
Personal Account B     900,000
Company Alpha        4,000,000
Company Beta         2,000,000
```

Possible paths may have different:

- Tax rates.
- Withdrawal fees.
- Transfer costs.
- Company distribution taxes.
- Salary/payroll taxes.
- Timing.
- Minimum-balance requirements.
- Working-capital effects.
- Future tax consequences.

The optimizer should test permitted strategies and compare them.

### 13.2 Optimization objectives

The user must choose or configure the objective.

Possible objectives:

- Minimize immediate tax.
- Minimize total tax over a period.
- Minimize tax + bank fees.
- Maximize net cash received.
- Preserve a minimum personal reserve.
- Preserve company working capital.
- Minimize future tax liability.
- Minimize number of transfers.
- Prefer/avoid specified banks.
- Prefer personal funds before company funds.
- Prefer company salary before dividend, or vice versa, where legal and user-configured.

The application should not assume that “minimum tax today” is always the optimal financial outcome.

### 13.3 Constraints

Examples:

- Never reduce Account A below 300,000.
- Never reduce Company Alpha below three months of payroll.
- Do not use Account B for this purchase.
- Only withdraw from Bank X after Bank Y.
- Maximum tax cost 100,000.
- Funds must be available by Nov 15.
- Preserve household emergency reserve of 1,000,000.
- Do not create a future cash deficit within the next 90 days.

### 13.4 Withdrawal/funding sequence

The user may configure account order:

```text
1. Account A
2. Account C
3. Company Alpha distribution
4. Account B
```

or allow the optimizer to search valid orders and rank them by the selected objective.

### 13.5 Explainable output

A tax-minimization result must show:

```text
Strategy 1
- Withdraw 645,500 gross from Account A
- Withdraw 400,000 gross from Account B
- Estimated tax: 42,000
- Bank fees: 3,500
- Net household cash: 1,000,000
- Account A ending balance: 854,500
- Account B ending balance: 500,000
- Reserve constraints: satisfied

Strategy 2
- Distribute 1,080,000 from Company Alpha
- Estimated distribution tax: 80,000
- Net household cash: 1,000,000
- Company working-capital reserve: satisfied

Preferred under objective “Minimize immediate tax + fees”: Strategy 1
```

These illustrative amounts are arithmetic examples, not outputs of an implemented tax solver. A real recommendation must establish net proceeds after actual cash deductions, final tax consequences, the evaluated search space and solution status. “Best” may mean best among enumerated candidates; it must not imply a global optimum unless supported by the model and solver certificate.

### 13.6 Legal tax minimization only

The engine should optimize within configured legal/tax rules. It should not contain features whose purpose is concealment, false reporting, evasion, or bypassing lawful obligations.

---

## 14. User-Defined Rules Engine

Adaptability is a major requirement.

The application should not require developers to hard-code every bank, jurisdiction, tax or household rule.

### 14.1 Rule structure

A rule should contain:

- Name.
- Scope.
- Trigger.
- Conditions.
- Action/calculation.
- Priority.
- Effective date range.
- Enabled/disabled state.
- Scenario applicability.
- Explanation text.
- Version/history.

### 14.2 Rule scopes

Rules may apply to:

- Household.
- Person.
- Company.
- Institution/bank.
- Account.
- Card.
- Category.
- Transaction type.
- Income type.
- Tax type.
- Scenario.

### 14.3 Example tax rule

The following tax rates and thresholds are fictitious rule-engine examples. They do not state the law of any country. A real rule must specify whether a threshold applies per transaction, per day, cumulatively, to the full amount or only the excess; whether withholding is creditable; and its official effective-dated source.

```text
Rule: DEMO cash withdrawal withholding
Scope: Personal bank accounts at Bank A
Condition: Withdrawal > 50,000
Rate: 0.6%
Effective: Jul 1 2026 – Jun 30 2027
```

### 14.4 Example credit-card rule

```text
Rule: DEMO foreign card tax
Condition:
- Payment method = credit card
- Transaction currency != account base currency
Action:
- Add 5% tax event
- Add 1.5% bank fee event
```

### 14.5 Example funding rule

```text
Rule: Car purchase funding
1. Use Shared Savings while preserving 1,000,000.
2. Then use Person A Savings while preserving 300,000.
3. Do not use Company Alpha unless purchase date is after Dec 1.
```

### 14.6 Example bank-selection rule

```text
For ordinary household expenses:
- Prefer Bank A debit account.
- Use Bank B only when Bank A would fall below 100,000.
- Never use Company accounts.
```

### 14.7 Rule conflict resolution

When multiple rules apply, the engine must deterministically resolve conflicts via:

- Explicit priority.
- Scope specificity.
- User-selected tie-breaking policy.

The conflict and resulting decision should be inspectable.

### 14.8 Rule simulation

Before activating or changing a rule, the system should eventually be able to show:

> “If this rule had been active, these projected balances/taxes would change.”

This makes powerful custom rules safer.

---

## 15. Transfers and Internal Movements

Transfers must be first-class events.

If 500,000 moves from checking to savings:

```text
Checking   -500,000
Savings    +500,000
Household net effect = 0
```

The system must not count the movement as both an expense and income.

### 15.1 Cross-boundary transfers

Transfers across economic boundaries can have real effects.

Example:

```text
Company Alpha -> Person A
```

This may be:

- Salary.
- Dividend/distribution.
- Reimbursement.
- Loan repayment.
- Owner draw.

Therefore the cross-boundary movement must carry a semantic type and tax treatment.

### 15.2 Credit cards

A normal credit-card flow should avoid double counting:

- Purchase transaction = expense.
- Credit-card liability increases.
- Paying the credit-card statement = transfer/liability settlement, not a second expense.

---

## 16. Planned vs Actual and Reconciliation

The system should separate planned financial events from actual transactions.

Example:

```text
Planned salary: 300,000 on Sep 30
Actual salary: 297,420 on Sep 29
```

The user/system should be able to link them.

Possible statuses:

- Planned.
- Due.
- Partially fulfilled.
- Fulfilled.
- Skipped.
- Cancelled.
- Overdue.

Once an event is fulfilled, future calculations should use the actual transaction and must not count the planned occurrence again.

This is essential for forecast correctness.

---

## 17. Reservations and Minimum Balances

A bank balance is not necessarily available money.

Example:

```text
Savings account balance       2,000,000
Emergency reserve               800,000
Tax reserve                     300,000
School fee reserve              250,000
------------------------------------------------
Potentially free                 650,000
```

Reservations should be modelled separately from accounts so several goals can use one account without requiring fake bank accounts.

The 650,000 example assumes the three earmarks are disjoint. If the emergency reserve already includes a bank minimum, that minimum is not deducted again. Each reserve records its coverage, nesting and whether it is a hard constraint or user-relaxable preference.

Rules may also impose balance constraints:

- Bank minimum balance.
- Company working capital.
- Payroll reserve.
- User-defined safety floor.

---

## 18. Scenario Engine

A scenario should be an overlay over the baseline rather than an entirely separate disconnected financial database.

### 18.1 Baseline

Represents the current best-known plan.

### 18.2 Scenario changes

A scenario may:

- Add events.
- Remove events.
- Change amounts.
- Change dates.
- End income streams.
- Add employment.
- Add/remove companies.
- Change tax rules.
- Change account funding rules.
- Change assumptions.

### 18.3 Scenario composition

Examples:

- Leave job.
- Buy car.
- Leave job + buy car.
- Start Company Beta.
- Extract 1,000,000 from Company Alpha.
- Move home + rent increase.

Scenarios should be combinable when their changes are compatible.

### 18.4 Comparison metrics

Useful comparison outputs include:

- Balance on selected dates.
- Lowest balance.
- Date of lowest balance.
- Reserve breaches.
- Total taxes.
- Incremental taxes.
- Fees.
- Debt.
- Cash runway.
- Company working capital.
- Business payroll coverage.
- Goal delays.


### 18.5 Scenario, assumption, goal and decision privacy

Scenarios are financial objects and require their own authorization boundaries. Visibility of underlying shared accounts must not automatically expose a private scenario.

Examples include privately modeling:

- Leaving a job.
- Starting or selling a company.
- Moving home.
- A personal purchase.
- A private debt-repayment plan.
- A change in expected income.

Scenario visibility, assumption visibility and permission to use restricted resources must be evaluated separately. A scenario may therefore be private to its creator while using only the creator's authorized data, or it may be shared for household planning under an explicit disclosure policy.

A private scenario must not leak through household comparison lists, notifications, audit summaries visible to unauthorized users, derived goal dates or differences between baseline and scenario calculations.

---

## 19. Major Purchase / Affordability Analysis

The application should not return a black-and-white yes/no answer.

Example decision:

```text
Car price: 8,000,000
Purchase window: Oct–Mar
Down payment: 2,000,000–5,000,000
Household emergency reserve: >= 1,000,000
```

### 19.1 Affordability metrics

Calculate at least:

- Immediate cash after purchase.
- Lowest projected cash after purchase.
- Date of lowest balance.
- Emergency reserve remaining.
- Future shortfalls caused by the purchase.
- Monthly repayment.
- Total financing cost.
- Free cash flow after purchase.
- Time to recover a target savings level.
- Goals delayed or made infeasible.
- Taxes/fees triggered by funding the purchase.
- Company cash consequences if business funds are considered.

### 19.2 Parameter search without AI

The application can deterministically evaluate combinations such as:

```text
Purchase month × down payment × loan term × funding source
```

For each valid combination it calculates the financial timeline.

The system can then rank the combinations under an explicit objective.

Example objective:

> Find combinations that never breach a 1,000,000 household reserve, preserve Company Alpha's payroll reserve, and minimize tax + financing cost.

### 19.3 Conditional affordability result

The result should be phrased as conditional on assumptions.

Example:

```text
November with a 2.5m down payment satisfies the configured safety rules.

This result depends on:
- Sep–Nov salary >= 1.42m combined.
- Client payment of >= 300k received by Nov 15.
- Rent <= 180k/month.
- No additional unplanned expense above 200k.
- Tax Rule Pack PK-2026-v3 remaining applicable.
```

---

## 20. Goals and Competing Uses of Money

A future expense does not exist in isolation.

The system should support goals such as:

- Emergency fund.
- Home purchase.
- Car purchase.
- Education.
- Wedding.
- Travel.
- Business capitalization.
- Debt payoff.

A scenario should expose trade-offs.

Example:

> Buying the car in November remains above the emergency reserve, but delays the house down-payment goal from March to July.

This is deterministic goal scheduling, not AI advice.

---

## 21. Important Edge Cases and Acceptance Scenarios

The following cases should be treated as requirements/acceptance tests for the domain model.

### Income and employment

- Person has five clients paying on different schedules.
- Client payment is late.
- One-time bonus.
- Commission varies within a user-defined range.
- Salary stops in three months.
- Salary changes on an effective date.
- Temporary unpaid leave.
- Employment ends mid-month.
- Final paycheck differs from ordinary salary.
- New job begins before/after old job ends.

### Household expenses

- Annual insurance premium.
- Quarterly school fees.
- Rent increases from a future date.
- Subscription changes price.
- Large one-time repair.
- Wedding with staged deposits/payments.
- Home move with deposit, old-deposit refund and new rent.

### Receivables/liabilities

- Money loaned to a friend.
- Friend repays in installments.
- Family loan received without conventional bank financing.
- Refund expected later.
- Tax refund expected.

### Accounts and transfers

- Transfer checking -> savings.
- Account minimum balance.
- Fixed deposit not immediately liquid.
- Cash outside a bank.
- Credit-card purchase + later statement settlement.
- Transfer between two household members.

### Company scenarios

- Person A owns two companies.
- Each company has separate payroll dates.
- Company has sufficient cash but insufficient working capital after payroll.
- Company owner wants to extract cash for household purchase.
- Owner salary versus distribution has different tax consequences.
- Company owes tax shortly after a proposed owner distribution.
- Company receives a major client payment late.
- Employee leaves in two months and payroll ends.
- New employee starts next month.

### Tax and bank rules

- Cash withdrawal tax applies only above a threshold.
- Credit-card tax applies to foreign purchases.
- Bank fee differs by account.
- Tax rate changes on a future effective date.
- User has two legal funding paths with different tax outcomes.
- Cheapest-tax path would violate the emergency reserve.
- Lowest-tax path would make company payroll unsafe.
- A higher immediate tax path results in lower total tax across the configured horizon.

### Major decisions

- Buy car now versus three months later.
- Pay cash versus finance.
- Different down payments.
- Pay off loan early versus preserve liquidity.
- Start a new business while one salary ends.
- Purchase funded partly from personal account and partly from company distribution.

### Uncertainty

- Salary is expected but not guaranteed.
- Rent may increase within a range.
- Client payment has amount and date ranges.
- One assumption fails and affordability changes.
- Conservative scenario fails while expected scenario succeeds.

### Privacy and authorization

- Person A owns a private savings account that is fully excluded from household calculations.
- Person A permits that account to contribute to a household affordability calculation without revealing its existence, institution, balance or transactions.
- Household member can see an authorized aggregate contribution but cannot derive the private balance from explanation details.
- Balance is shared while transactions and merchant/category history remain private.
- Joint economic ownership remains 50/50 even when visibility permissions differ.
- An access grant begins or ends on a future effective date and forecast authorization changes accordingly.
- A private account is authorized for `Buy Home` but not for `Buy Car`.
- A private scenario such as `Leave Job` is invisible to other household members and does not leak through notifications or scenario-difference summaries.
- Company accountant can inspect payroll while household members can only see an authorized contribution from the owner.
- Employee salary movement remains one linked economic event although each viewer sees a different authorized representation.
- Revoking calculation access removes the resource from new shared calculations without altering historical accounting truth.
- Historical calculation replay preserves the policy version used at calculation time.
- A difference attack that would reveal one restricted account through subtraction is suppressed, aggregated or explicitly authorized.
- Export/API access cannot bypass the same visibility and calculation-access policy used by the interactive application.

If these cases can be represented without model-specific hacks, the architecture is likely robust.

---

## 22. Deferred and Intentionally Unspecified Items

This requirements document deliberately avoids deciding implementation sequencing. **No functional requirement is removed merely because it appears in this list.** “Deferred” here means the specification intentionally does not yet prescribe the detailed design, provider, jurisdiction, threat model, workflow, or delivery structure.

### 22.1 Explicitly deferred

- **User-experience and visual/interface design.** Screen layout, navigation, dashboards, interaction patterns, information density, chart selection, onboarding, and final terminology will be designed after the requirements set is complete.
- **Detailed privacy-configuration UX.** The authorization architecture, policy semantics, permission-aware provenance, and enforcement rules are requirements now; the final UI for configuring those controls is deferred.
- **Task decomposition and delivery structure.** Work breakdown, implementation stages, milestones, release sequencing, MVP definition, feature prioritization, engineering tickets, sprint planning, and launch scope are intentionally absent from this document and will be decided separately.
- **Implementation stack and vendor selection.** Programming language, frameworks, database, hosting, solver vendor, bank-data provider, market-data provider, document/receipt provider, and deployment topology are not selected here.
- **Jurisdiction-specific production tax packs until jurisdictions are selected.** The tax engine, tax-rule model, versioning, provenance, and optimization requirements are included now. Exact statutory rules must be implemented and verified separately for each supported jurisdiction, filing unit, tax year, and legal entity type.
- **Final privacy threat model beyond application authorization.** Secure multi-party computation, homomorphic encryption, differential privacy, trusted execution environments, and similar cryptographic/privacy-preserving computation techniques are conditional research items unless the eventual threat model requires them.
- **Transaction-execution workflows.** This document specifies planning, recommendation, and analysis. Any capability that actually transfers money, submits payments, trades assets, files taxes, or otherwise executes a financial action requires a separate authorization, security, regulatory, failure-recovery, and audit specification.
- **Provider-specific bank connectivity details.** Direct synchronization remains in the product requirements inventory, but supported institutions, connection providers, refresh schedules, webhook/reconciliation behavior, and jurisdictional coverage are not selected here.
- **Receipt/document-capture implementation details.** Receipt capture may exist as a module, but OCR/provider selection, retention policies, extraction schema, and correction UX are not specified here.
- **Statutory filing and reporting integrations.** Tax calculation and planning are requirements; submission formats, filing APIs, signatures, certifications, and jurisdiction-specific compliance workflows require separate validation.
- **Final numerical-solver strategy per model.** Applicable mathematical models, evidence requirements, tolerances, replay checks, and solution-status reporting are specified; the exact solver, decomposition technique, fallback strategy, and licensing choice remain open.
- **Final data-source policy for investments, FX, rates, inflation, and reference data.** The models must preserve source/version/timestamp provenance; specific providers and refresh policies are intentionally unspecified.

### 22.2 Explicitly not deferred

The following are foundational requirements even though their eventual UI or implementation sequencing is unspecified:

- Deterministic and inspectable calculation logic.
- Complete calculation provenance with authorization-aware disclosure.
- Separation of current money from uncertain future money.
- Assumption-bound forecasts and explicit uncertainty modeling.
- Multiple people, accounts, companies, ownership boundaries, and linked cross-entity events.
- Privacy/authorization architecture and effective-dated policy enforcement.
- Tax calculation architecture, tax-rule versioning, and lawful tax-aware funding analysis.
- User-defined rules and deterministic rule evaluation.
- Household/company boundary preservation.
- Scenario, affordability, funding, liquidity, risk, and optimization requirements contained elsewhere in this document.
- Verification, replay, numerical tolerances, and acceptance-test requirements.
- No-AI constraint for calculations, classifications, forecasts, scenario comparisons, and recommendations.

---

## 23. Scope Boundaries Without Premature Feature Pruning

All relevant deterministic mathematical and financial capabilities are eligible for the discovery inventory, including portfolio analysis, sophisticated taxation, business working capital, insurance, lifetime planning and constrained optimization. Inclusion is a requirement candidate, not a promise of simultaneous implementation.

**Prohibited in this specification:** AI/ML classification, AI-generated financial assumptions, LLM explanations or recommendations, concealed inference, tax evasion, fabricated records and silently executed financial transactions.

**Deferred/unspecified items are listed comprehensively in Section 22.** Privacy architecture is not deferred: every relevant financial object must have an authorization boundary, and economic ownership, visibility, calculation access and disclosure must remain distinct. Basic authentication, authorization enforcement, data integrity, safe imports and protection against data loss are foundational engineering requirements.

**Separate modules requiring separate validation:** bank connectivity, receipt capture, statutory reporting, filing integrations and payment execution. These may remain in the full product inventory, but no such capability or regulatory compliance is implied by a working planning engine. Any execution module requires explicit authorization and its own controls; recommendations alone do not move money.

---

## 24. Calculation Provenance Requirement

Every calculation result should be reproducible from stored data.

A derived result should have enough metadata to reconstruct:

```text
Result
├── calculation formula / algorithm version
├── current balance snapshot
├── event occurrences used
├── rules applied
├── tax-rule versions applied
├── assumptions applied
├── scenario overrides
├── excluded items
├── authorization-policy versions
├── calculation purpose/context
├── full internal calculation graph
├── viewer-specific disclosure rendering
└── timestamp
```

This supports:

- Trust.
- Debugging.
- Auditability.
- Tax-rule updates.
- Historical comparison.
- Correct scenario reproduction.
- Permission-aware historical replay.
- Demonstrating that a displayed explanation contains no unauthorized source data.

---

## 25. Rule and Calculation Versioning

Determinism is only useful if calculations remain reproducible after rules change.

Therefore:

- Rules should be versioned.
- Tax rule packs should be versioned.
- Forecasts should record which version was used.
- Editing a rule should not silently rewrite historical actual results.
- Users should be able to re-run future scenarios under a newer rule set.

Example:

```text
Scenario originally calculated using:
DEMO-JURISDICTION-2026-v2

New tax rules available:
DEMO-JURISDICTION-2027-v1

Future scenarios may be recalculated; original results retain the original rule version.
```

---

## 26. Deterministic Recommendation Contract

Any recommendation produced by the application should conceptually contain:

```text
Recommendation
├── action being recommended
├── objective being optimized
├── candidate strategies evaluated
├── constraints
├── winning strategy
├── metrics for winning strategy
├── alternatives
├── assumptions
├── applied rules
└── explanation
```

Example:

```text
Recommendation:
Consider buying the car in November with a 2.5m down payment.

Objective:
Minimize tax + financing cost while maintaining >= 1m household reserve.

Why:
- Lowest projected household cash: 1.24m
- Total funding tax/fees: 83k
- Company payroll reserve: maintained
- No account becomes negative

Alternatives tested: 42
Feasible alternatives: 11

Key assumptions:
- Person A salary remains >= 480k/month through Nov
- Person B remains employed through Nov
- Client receivable >= 300k arrives by Nov 15
- Rent remains <= 180k/month
```

This is not an AI recommendation. It is the reported result of deterministic search under user-defined inputs.

---

## 27. Suggested Core Data Entities

A conceptual schema should include at least:

- Household.
- Person.
- Company.
- Company ownership.
- Employee/employment.
- Account.
- Account ownership.
- Balance snapshot.
- Actual transaction.
- Transfer.
- Event series.
- Event occurrence.
- Assumption.
- Reservation.
- Goal.
- Scenario.
- Scenario override.
- Rule.
- Rule version.
- Tax rule.
- Tax rule pack/version.
- Tax liability.
- Funding policy.
- Funding strategy/result.
- Purchase/decision plan.
- Financing plan.
- Receivable.
- Liability/debt.
- Reconciliation link.
- Calculation result/provenance.
- AccessPolicy.
- AccessGrant.
- HouseholdRole.
- EntityRole.
- DisclosurePolicy.
- PrivacyAuditEvent.
- Authorization context/purpose.

Policy references should be attachable to at least Person, Company, Account, Transaction, Event Series, Event Occurrence, Income/compensation records, Reservation, Goal, Scenario, Assumption, Receivable, Liability and Calculation Result.

The schema should be centred around events and entities rather than an “expense” table.

---

## 28. Delivery Structure Intentionally Unspecified

This document contains **no prescribed work breakdown or implementation sequence**. Requirements are deliberately presented as one complete discovery inventory. Relationships between mathematical models, financial entities, rules, data, and verification obligations may be recorded where technically necessary, but those relationships must not be interpreted as delivery order or release priority.

Delivery planning will be performed separately after requirements analysis.

---

## 29. Product-Research Foundations Carried Forward

The earlier product-research baseline identified the following useful patterns. These are retained requirements inspirations, not a fresh audit of every current product feature; see the carried-forward product references in Section 47:

1. **Withdrawal order materially affects tax outcomes.** RightCapital explicitly models different withdrawal sequences and compares their effects.
2. **Tax scenarios should be compared against a baseline.** Holistiplan calculates the incremental tax effect of proposed actions and supports scenario comparison.
3. **Optimization needs an explicit objective and constraints.** ProjectionLab's newer optimization tooling lets users select tax-planning objectives and strategy constraints.
4. **Tax details should remain inspectable.** Existing advanced planning products expose account withdrawals, tax effects and strategy details rather than only a recommendation.

The new application should generalize these patterns beyond retirement accounts into ordinary bank accounts, company extraction, local taxes/withholding, card/withdrawal taxes, fees, reserves and household liquidity.

---

## 30. Core Product Requirement

The product can be summarized as:

> **Create a user-controlled financial timeline that combines every person's and company's authorized current financial state, known and assumed future inflows/outflows, tax and funding rules, obligations and hypothetical decisions, and allows each viewer to inspect how a decision changes financial position at the greatest level of detail they are authorized to receive—without requiring household participants to surrender unrelated private financial data.**

Two statements should remain explicit requirements throughout implementation:

> **Every derived value must expose its inputs and calculation.**

> **All financial calculations, classifications, projections, scenario comparisons and recommendations must be produced by deterministic, inspectable logic.**

And the forecasting philosophy should be:

> **Future money may participate in a forecast, but it is never silently treated as money already available. Every future-dependent conclusion is conditional on a visible set of assumptions.**

---


<a id="research-method"></a>
## 31. Research Method and Interpretation

### 31.1 Evidence policy

The research prioritizes original papers, author-hosted manuscripts, original institutional reports and official documentation. The bibliography distinguishes material inspected in full or in relevant sections from material for which only the publisher/author abstract or bibliographic record was accessible. An inaccessible paper is not represented as fully reviewed.

Three different things must remain distinct:

| Label | Meaning in this document |
|---|---|
| **Published method** | A mathematical construction or finding attributable to the linked source |
| **Proposed application requirement** | An original adaptation of that method to this household/company product; not something the paper necessarily implemented or proved |
| **Implementation evidence** | Tests, independent replay, benchmarks and certificates still required of the actual software |

References justify considering a method. They do not certify the future application, establish local tax law, or prove that its input assumptions are true. Where a paper uses simplified taxes, frictionless transfers or known future returns, those assumptions must not silently enter this application.

### 31.2 Strongest research connections

| Research connection | Requirement implication | Evidence |
|---|---|---|
| Robust optimization with an explicit uncertainty budget | Test affordable decisions against stated joint deviations rather than treating three scenarios as a proof | [R01](#r01) |
| Conditional value-at-risk and distributional ambiguity | Distinguish breach frequency, severity and uncertainty about probabilities | [R02](#r02), [R03](#r03), [R04](#r04) |
| Multi-period optimization and model predictive control | Plan across future dates but revise the plan using information actually available at each decision | [R05](#r05), [R06](#r06) |
| Personal-finance stochastic optimization | Model tax/account decisions jointly for individuals and couples | [R08](#r08) |
| Cash management as a constrained network | Represent multiple accounts, costs, legal transfer paths and business liquidity | [R09](#r09), [R10](#r10) |
| Multi-objective cash policy and liability matching | Compare cost, stability and future obligation coverage instead of one opaque score | [R11](#r11), [R12](#r12) |
| Tax-aware portfolio construction | Make tax basis, lots and realized gains first-class planning state | [R07](#r07) |
| Lifecycle costing and real options | Compare complete ownership costs and the consequences of waiting | [R15](#r15), [R17](#r17) |

### 31.3 Replacement policy

Do not replace correct simple identities merely because a more elaborate formula exists. Use the simplest model that is **valid for the specified contract and question**, and use a more general model when the assumptions of the simple one fail.

| Earlier simplification | Required replacement or generalization |
|---|---|
| One household balance | Per-entity/account/currency/availability state plus explicit aggregation boundaries |
| Balance minus every reserve | Constrained liquidity with non-overlapping earmarks and explicit nested floors |
| Current balance plus monthly net income | Event-time state evolution with settlement, taxes, rules and conditional paths |
| Conservative / expected / optimistic proves safety | Named scenarios plus joint uncertainty sets; separate robust and probabilistic analyses |
| Cash divided by average expenses | First funding-breach date and minimum additional capital over a dated timeline |
| Cheapest withdrawal rate first | Joint multi-account, multi-entity, multi-period net-funding optimization |
| Tax equals amount times rate | Effective-dated rule graph with brackets, credits, withholding, carryforwards and payment timing |
| Fixed loan-payment formula handles all borrowing | Contract-level amortization; annuity formula retained only for its valid special case |
| Grid search finds the optimum | Search-space disclosure, discrete resolution and solver/certificate status |
| Smallest tax bill is the best decision | User-selected objectives with after-tax cost, liquidity, risk, timing and goals |
| Future plan knows which invoice will pay | Nonanticipative policies and explicit information-revelation dates |
| A mathematically optimized answer is guaranteed in reality | Conditional model statement, execution constraints and uncertainty-coverage limitations |

<a id="mathematical-contract"></a>
## 32. Mathematical Contract and Notation

### 32.1 Shared notation

| Symbol | Meaning |
|---|---|
| $e,a,c$ | Legal entity, account and currency |
| $t$ | Event/settlement time, not necessarily an equally spaced month |
| $s\in S$ | A complete scenario path, not an isolated monthly outcome |
| $b_{a,t}$ | Settled cash balance in account $a$'s native currency |
| $z_t$ | Complete state: cash, debt, tax basis, carryforwards, receivables, reserves and obligations |
| $x_t$ | Decision made at time $t$, subject to information and legal constraints |
| $\xi$ | Uncertain amounts, dates, rates, employment, collections and other assumptions |
| $\mathcal U$ | Explicit joint set of allowed uncertainty paths |
| $p_s$ | User-approved probability for scenario $s$, only when probabilistic analysis is enabled |
| $R_t$ | Required reserve for a specified pool and coverage definition |
| $D_t$ | Discount factor to the valuation date; debt balances use $B_t^{debt}$ below |
| $[v]_+$ | $\max(v,0)$ |
| $H$ | Stated planning horizon |

Amounts may not be added across currencies without a declared conversion convention. Rates carry a period and compounding/day-count convention. A value in currency-days is not a value in currency. Native cash calculations use integer minor units or an appropriate decimal type with rule-defined rounding.

### 32.2 Required result-strength labels

| Label | Permitted claim | What it does not establish |
|---|---|---|
| **Exact accounting calculation** | Reconciled postings and configured rounding produce the stated balance | The imported data is complete or the future events will occur |
| **Conditional path calculation** | A specified path produces the stated outcome | Other paths are safe |
| **Scenario-tested** | Every named path in a finite set passed | Unexamined paths or a continuum of values passed |
| **Robust-feasible under U** | All model constraints hold for the complete declared uncertainty set, under a validated robust method | Reality will remain in that set |
| **Probability-model result** | A probability or tail metric follows from the disclosed probability model | The model is the true future distribution |
| **Best on specified grid** | No tested combination on that finite grid was better | A continuous or untested combination cannot be better |
| **Solver-certified to stated tolerances** | The mathematical program meets the recorded feasibility/optimality criteria | Exact arithmetic proof or correct economic/legal assumptions |
| **Feasible candidate / approximation** | A candidate passed the replayed checks | Global optimality or robust feasibility outside the checked set |
| **Unresolved / incomplete model** | Inputs or supported rules are insufficient for a claimed answer | That the real-world decision is necessarily impossible |

A valid robust statement has the logical form:

$$
\bigl(\xi_{actual}\in\mathcal U\bigr)
\land \bigl(\text{model/rules are valid}\bigr)
\land \bigl(\text{actions execute as modeled}\bigr)
\Rightarrow \text{stated constraints hold through }H.
$$

It must never be shortened to “guaranteed affordable.” Solver tolerances and model-form conditions must also accompany the statement. Numerical optimization documentation explicitly makes finite-precision limits relevant to interpreting solver results. [R28](#r28)

### 32.3 No-AI compatibility

Linear programming, mixed-integer programming, convex optimization, dynamic programming, robust optimization, actuarial arithmetic, statistical summaries and numerical integration are allowed mathematical tools. Their assumptions, inputs and algorithms must be explicit. None requires AI.

The default uncertainty mechanisms are user-authored paths and sets. Optional probability modules require user-approved distributions. Random sampling is not a source of financial assumptions: a simulation must use a stored seed, generator, scenario sample and algorithm version so it can be replayed. Prefer exact finite-path evaluation when practical. An ML-trained forecasting model or LLM-produced explanation is outside this specification.

<a id="model-catalogue"></a>
## 33. State, Accounting and Transfer Mathematics

### M01 — Balanced event ledger and full state evolution

**Proposed requirements.** Represent actual transactions with balanced entries; represent planned transactions as conditional future entries. Keep cash and non-cash state distinct. Preserve reversals, reconciliation, tax liabilities and in-transit transfers without overwriting history.

$$
b_{a,t}=b_{a,t^-}+\sum_{j:\,settles(j)=t}\Delta b_{a,j},
\qquad z_t=F_t(z_{t^-},x_t,\xi_t;\mathcal R_t).
$$

Here $\mathcal R_t$ is the applicable rule snapshot. The state function also updates debt principal, tax basis, withholding credits and unsettled obligations. For a balanced journal in its defined accounting unit, total debits equal total credits. Foreign-exchange trades require appropriate currency/clearing and valuation entries, not cancellation of unequal currency numbers.

**Required outputs.** Account cash, pending cash, available cash, accrual income/expense, liabilities and an event-by-event explanation. Taxes and fees enter cash once as actual postings; the forecasting engine must not subtract them again.

**Boundary.** The conservation identities are exact under recorded entries. This does not turn a planning ledger into certified statutory accounts. The cash/non-cash distinction is consistent with IAS 7; the proposed implementation is not an IFRS compliance claim. [R19](#r19)

### M02 — Spendable liquidity as a constrained quantity

For a single immediately accessible pool with disjoint earmarks:

$$
U_t=b_t-\sum_k r_{k,t}.
$$

For multiple accounts, define decision-specific available funding instead:

$$
A_t(q)=\max_{x\in\mathcal X_t}\{\text{net settled funds delivered for purpose }q\},
$$

where $\mathcal X_t$ enforces ownership, access, reserves, account restrictions, fees, taxes and deadlines. A company account is not included merely because a person owns shares.

**Proposed features.** Separate ledger cash, accessible cash, earmarked cash, credit availability and lawful extractable cash. Support earmark coverage sets and independent versus nested floors. If a 100,000 bank minimum is already inside a 300,000 emergency reserve, their combined requirement is 300,000, not 400,000. Distinct obligations may be additive.

**Limits.** “Available for car purchase” can differ from “available for payroll” on the same date. Negative headroom must not disappear when a spendable display is floored at zero. Multi-account cash-system research motivates the constraint-based approach. [R10](#r10)

### M03 — Time-expanded funding and transfer network

Use nodes $(e,a,c,t)$, with arcs for retaining funds, transferring them, converting currency, withdrawing from investments, paying obligations and lawfully distributing company cash. An arc has a source debit, destination receipt function, delay, capacity and eligibility.

For each node:

$$
\text{opening cash}+\text{external receipts}+\sum_j \operatorname{received}_j(x_j)
=\text{closing cash}+\text{external payments}+\sum_k \operatorname{debited}_k(x_k).
$$

Fixed fees may require a binary activation $y_k$, with $0\le x_k\le U_k y_k$. Progressive taxes and threshold fees need their actual piecewise/discrete rules, not a constant arc multiplier.

**Proposed features.** Cheapest lawful funding route, transfer-date scheduling, withdrawal limits, settlement delays, deposit lockups, grace periods, FX routes, account-specific shortages and required prefunding. Prevent apparent tax arbitrage, profitable cycles caused by inconsistent exchange quotes, and spending the same unsettled transfer twice.

**Evidence/limits.** Cash-flow research explicitly uses network and mixed-integer robust models. The household/entity/time graph here is a proposed generalization, not a pre-proven implementation. [R09](#r09), [R10](#r10)

### M04 — Ownership, economic exposure and consolidation

$$
NW_{boundary}=\sum \text{included asset values}-\sum \text{included liabilities},
$$

with an explicit valuation/consolidation basis. A household view may show a person's share of a company's estimated equity **or** a consistently consolidated set of underlying assets and liabilities. It must not add both.

**Proposed features.** Separate legal ownership, economic exposure, voting/control information, distribution rights and eligible cash extraction. Eliminate matching internal flows only within a selected analytical boundary. Track third-party shareholders, shareholder loans, dividends, salary and reimbursements separately.

**Tests.** An owner-company payment must affect each standalone ledger, eliminate once in a combined cash-flow view, and still preserve taxes and payments to outside employees. A company valuation remains an illiquid asset, not personal cash.

**Boundary.** A convenience “household plus companies” analysis is not automatically legal tax consolidation or IFRS consolidation; IFRS 10 uses a control-based reporting framework. [R21](#r21)

### M05 — Financial clocks, recurrence and settlement order

Define the future occurrence stream as:

$$
\mathcal E=\operatorname{Expand}(\text{series},\text{exceptions},\text{calendar},\text{effective versions}).
$$

Each occurrence distinguishes contractual due date, service/accrual period, posting date, settlement date and availability time. Financial calendars add bank holidays, cutoff times and contract-specific month-end adjustment to general calendar recurrence.

**Proposed features.** Last business day salary, fortnightly payroll, four-week schedules, leap years, final partial salaries, skipped payments, three-paycheck months, contract anniversary price changes, non-business-day adjustments and correction history. Specify whether an invalid monthly date is skipped, clamped or moved; do not silently inherit a calendar library's default.

**Tests.** A rent debit at 09:00 and salary at 17:00 can cause a real shortfall despite positive end-of-day cash. Two-day settlement cannot fund today's payment. RFC 5545 is a recurrence reference, not a banking-calendar specification. [R27](#r27)

### M06 — Currency-aware valuation and funding

For a reporting currency $c_0$:

$$
V_t^{c_0}=\sum_{a} b_{a,t}^{c(a)}FX_{c(a)\rightarrow c_0,t}.
$$

This is a valuation only. Executable conversion uses the actual side of the quote, spread, minimum/fixed fees, taxes, lot size and settlement date. Define whether quoted rates are destination units per source unit.

**Proposed features.** Multi-currency salaries and invoices, remittance comparisons, FX stress, contractual exchange rates, split currency goals and currency-specific reserves. Show realized versus unrealized exchange effects and tax-reporting exchange conventions separately.

**Limits.** A marked-to-market foreign balance is not necessarily convertible at that price or by the required date. Netting exposures does not imply permission to net payments across legal entities. Network-based cash planning and liquidity-risk principles support modeling execution restrictions explicitly. [R09](#r09), [R26](#r26)

## 34. Uncertainty, Affordability and Risk Mathematics

### M07 — Coherent joint paths and dependency factors

Model uncertainties as a joint path rather than independent monthly sliders:

$$
\xi=\bar\xi+L f,\qquad f\in\mathcal F,
$$

where $f$ contains user-defined shared factors and $L$ maps them into income, costs and timing. This factor representation is an optional structural model, not a fitted AI predictor.

**Proposed features.** A late Company Alpha client payment can simultaneously delay Alpha payroll and its owner's household salary; rent and school fees can respond to a common inflation assumption. Two businesses with the same customer can fail together. Model salary continuity as an employment-state event, not merely a narrow amount range.

**Requirements.** Separate impossible combinations from allowed stresses. Store scenario histories, date dependencies, shared causes and assumption provenance. A named “optimistic” or “conservative” path is descriptive; the optimizer must not combine favorable facts from incompatible paths.

**Evidence.** Robust and multistage planning motivate explicit joint uncertainties. Application factor choices and their plausibility remain user/modeler responsibilities. [R01](#r01), [R08](#r08)

### M08 — Robust affordability over a declared uncertainty set

A generic robust decision problem is:

$$
\min_{x\in\mathcal X}\ \sup_{\xi\in\mathcal U} C(x,\xi)
\quad\text{subject to}\quad g_j(x,\xi)\le0\quad\forall\xi\in\mathcal U,\ \forall j.
$$

The objective can instead minimize purchase delay while constraining worst-case cash, payroll and taxes. A feasibility question need not optimize a cost at all.

**Proposed features.** Robust purchase-date/down-payment ranges; minimum safe opening capital; protected household and company floors; user-defined uncertainty-set versions; explicit worst-case witnesses and binding assumptions.

**Guarantee boundary.** Checking a few endpoints is sufficient only when the relevant mathematical structure proves it sufficient. Nonlinear rules, threshold taxes, options and date changes can invalidate corner shortcuts. A partially solved adversarial search cannot certify the full set. Robustness protects only against modeled uncertainty, not unbounded emergencies or incorrect rules. [R01](#r01)

### M09 — Budgeted uncertainty and the price of additional safety

One useful set is:

$$
\mathcal U_\Gamma=\{\bar\xi+d\odot u:\ |u_i|\le1,\ \sum_i|u_i|\le\Gamma\}.
$$

$\Gamma=0$ gives the nominal coefficients; larger $\Gamma$ permits more joint deviation. It is **not** a confidence percentage.

For a supported uncertain linear constraint, a robust counterpart can use:

$$
\bar a^T x+\Gamma\theta+\sum_i p_i\le b,\qquad
\theta+p_i\ge d_i|x_i|,\quad \theta,p_i\ge0.
$$

**Proposed features.** Compare the additional reserve, delayed purchase date or financing cost required when more things may go wrong together. Define uncertainty budgets across the complete path or factor groups; do not reset a budget inconsistently each day.

**Limits.** This particular counterpart assumes the stated affine coefficient uncertainty. Tax cliffs, uncertain event dates and non-affine state changes need other reformulations or explicit scenario branches. The budget and dependence assumptions must be chosen, not inferred as “safe.” [R01](#r01)

### M10 — Joint chance constraints, only with explicit probabilities

$$
\Pr\{b_{a,t}\ge R_{a,t}\ \text{for all required }a,t\le H\}\ge1-\varepsilon.
$$

This is a **whole-plan** probability constraint. A 95% pointwise condition on each of many dates is not a 95% probability that the entire plan survives.

**Proposed features.** Optional model-based probability of any reserve breach, account failure, payroll miss or goal shortfall. Store probability source, dependence model, sample construction and horizon. For finite paths, calculate exact weighted breach totals; paths with arbitrary stress weights do not acquire probabilities merely because the weights sum to one.

**Limits.** Distinguish uncertainty in the estimated probability from randomness under the model. An optimization based on a small sample can overfit its scenarios; independent validation and distributional-sensitivity analyses are required. Joint probabilities can change when correlations change even if marginal salary/rent distributions do not. [R04](#r04), [R08](#r08)

### M11 — Conditional value-at-risk for the severity of bad outcomes

For loss $L_s$, approved probabilities $p_s$ summing to one and $0<\alpha<1$:

$$
\operatorname{CVaR}_\alpha(L)=
\min_\eta\left[\eta+\frac{1}{1-\alpha}\sum_s p_s[L_s-\eta]_+\right].
$$

**Proposed features.** Compare the tail severity of maximum funding deficits, unpaid payroll, unmet goals or total cost—not only whether a failure occurs. Support CVaR limits or a user-defined cost/risk trade-off. Always state the loss variable and units.

**Important detail.** For discrete distributions, “average losses greater than VaR” can be wrong because probability mass at the threshold may need partial inclusion. The variational formula handles that issue.

**Limits.** CVaR is not a worst-case bound, and a small expected tail loss does not mean no serious event is possible. Without meaningful probabilities, use worst-case or scenario shortfall metrics instead. [R02](#r02), [R03](#r03)

### M12 — Distributionally robust planning

When probability estimates themselves are uncertain:

$$
\min_x\sup_{Q\in\mathcal P}\mathbb E_Q[L(x,\xi)],
\qquad
\mathcal P=\{Q:W(Q,\widehat P_N)\le\epsilon\}.
$$

$W$ is a declared Wasserstein distance; the radius, norm and scaling between currency amounts, dates and other factors must be specified.

**Proposed features.** Optional ambiguity-aware withdrawal/purchase plans; comparison against ordinary expected-cost plans; sensitivity to uncertainty about the distribution; explicit worst-case distribution summaries where available.

**Guarantee boundary.** The published finite-sample results require particular sampling, tail and mathematical assumptions and a correctly calibrated ambiguity set. Six salary observations do not automatically justify a statistically guaranteed future-income model. A valid bound on expected loss also does not automatically imply a no-bankruptcy guarantee. Apply a tractable reformulation only where its conditions hold; otherwise report approximation or unsupported model. [R04](#r04)

### M13 — First-passage runway and minimum additional funding

For a specified pool/path:

$$
\tau=\inf\{t\le H:b_t<R_t\},\qquad S_t=[R_t-b_t]_+.
$$

If flows are fixed, cash is one fungible pool and an immediate injection has no cost or downstream effect:

$$
K^*=\max_{t\le H}S_t.
$$

**Proposed features.** Date of first funding breach; depth of worst deficit; date of recovery; time spent below a floor; minimum extra capital; required transfer timing by account. Report “no breach through H,” not “infinite runway.”

**Metrics distinction.** Maximum deficit is currency. Integrated shortfall $\sum_t S_t\Delta t$ is currency-time and measures duration/severity, not repeated new capital required. One persistent 100,000 gap over ten days does not require 1,000,000 of fresh capital.

**Limits.** When taxes, transfers, FX, legal entities or interest depend on injection timing/size, solve the constrained funding problem instead of using the closed form. Cash-matching and liquidity-stress research motivate the dated approach. [R12](#r12), [R26](#r26)

### M14 — Reverse stress and minimax regret

Find the smallest defined shock that breaks a plan:

$$
\min_{\delta\in\mathcal D}\|W\delta\|
\quad\text{subject to a stated funding/goal constraint being breached}.
$$

Weights $W$ make a day of delay and a currency amount comparable only under an explicit user-approved scaling. Discrete “job ends” events require a corresponding shock-cost convention. Define a cash breach as at least one minor unit below the floor, or use an explicit numerical margin; a strict-inequality formulation may have an infimum rather than an attained minimum.

For users who prefer not to assign probabilities:

$$
\min_x\max_s\left[C(x,s)-\min_{y\in\mathcal X_s}C(y,s)\right].
$$

This is minimax regret against an explicitly defined scenario-wise hindsight benchmark.

**Proposed features.** Joint failure explanations; nearest assumption violation; “what would make November unsafe?”; plans that limit the cost of choosing wrongly across scenarios.

**Limits.** A hindsight benchmark knows the scenario and is not an executable recommendation. Single-variable breakpoints hold other variables fixed. Non-monotone tax/fee rules require enumeration or appropriate global analysis rather than blindly applying binary search. These are proposed robust decision-analysis extensions. [R01](#r01), [R25](#r25)

### M15 — Empirical calibration without invented certainty

For historical actuals $y_i$ and forecasts made before observing them $\widehat y_i$:

$$
MAE=\frac1n\sum_i|y_i-\widehat y_i|,
\qquad Coverage=\frac1n\sum_i\mathbf1\{l_i\le y_i\le u_i\}.
$$

**Proposed features.** Forecast-versus-actual error, interval coverage, directional bias, collection-delay summaries, scenario assumption misses and data freshness. Preserve the original forecast rather than recalculating it using later knowledge. Group errors by income source, company, event type and horizon.

**Limits.** Sample coverage is a descriptive statistic, not a future success probability. Backtests must separate fitting/choice data from evaluation data and account for structural changes such as resignation, new clients or a changed tax regime. Zero observations means unknown, not zero risk. User-defined deterministic quantiles or averages can suggest assumptions, but the user must approve them. Research on data-driven optimization highlights out-of-sample failure when decisions are evaluated on the same information used to choose them. [R04](#r04)

## 35. Multi-Period Decisions, Goals and Trade-offs

### M16 — Nonanticipative scenario-tree decisions

$$
x_t(s)=x_t(s')\quad\text{whenever scenarios }s,s'\text{ have identical observed history at decision time }t.
$$

**Proposed features.** Conditional purchase policies, job-change contingency plans, installment choices, future refinancing decisions and company distribution policies. An action may change after an invoice settles, not before the application could know which collection outcome occurred.

**Required state.** An information-revelation date for each uncertain event; the observable history associated with every branch; locked commitments versus reversible choices.

**Tests.** Two scenarios that are indistinguishable on November 1 must share their November 1 action. “Buy now only in the branch where December salary eventually arrives” is invalid unless that salary outcome is already known.

**Evidence/limits.** Multistage personal-finance optimization provides a relevant precedent. The application's dates, legal actions and information structure still require explicit modeling. [R08](#r08)

### M17 — Receding-horizon planning / model predictive control

At each decision date:

$$
\text{observe }z_t\ \rightarrow\ \text{solve the future plan}\ \rightarrow\
\text{recommend only the current authorized action}\ \rightarrow\ \text{observe again}.
$$

**Proposed features.** Re-plan after salary changes, client delays, tax-rule revisions, bank balance reconciliation or market movement. Preserve the prior plan and explain the changed recommendation. Frozen commitments must remain commitments in later runs.

**Limits.** A sequence of sensible re-plans is not automatically a globally optimal lifelong policy. The terminal assumptions and remaining horizon matter. No action is automatically executed under this requirements document.

**Evidence.** Published multi-period portfolio and 2025 retirement-funding work use this update-plan-act structure. Their simplified return/tax assumptions are not adopted as universal truths, and their reported simulations do not guarantee this application's outcomes. [R05](#r05), [R06](#r06)

### M18 — Dynamic programming and time-consistent risk

For a supported state/action model:

$$
V_t(z)=\min_{a\in A_t(z)}\{c_t(z,a)+\rho_t[V_{t+1}(F_t(z,a,\xi))]\}.
$$

$\rho_t$ is an explicit conditional expectation, worst-case operator or supported conditional risk measure. Specify the terminal value and complete state needed to make future evolution depend on the current state.

**Proposed features.** Sequential purchase decisions, flexible retirement spending, renew-versus-replace choices, staged company investments and optional deferrals.

**Limits.** A single static tail-risk objective does not automatically create a time-consistent policy after new information arrives. Nested risk formulations and the information structure require validation. State discretization introduces approximation; report grid resolution, bounds and sensitivity. Large state spaces may require decomposition rather than a claim of exact dynamic programming.

**Evidence.** Risk-averse dynamic-programming research establishes relevant constructions under defined Markov/risk assumptions. Household adaptation is proposed here. [R32](#r32)

### M19 — Pareto frontiers and lexicographic decisions

For cost, risk, purchase delay and goal shortfall $f_1,\ldots,f_k$, a plan is dominated if another is no worse in all selected objectives and better in at least one.

A useful search is:

$$
\min_x f_1(x)\quad\text{subject to}\quad f_j(x)\le\epsilon_j,\ j=2,\ldots,k.
$$

**Proposed features.** Nondominated alternatives; “lowest tax while preserving 1m”; “earliest purchase with payroll protected”; ranked goals; trade-off cost of raising a reserve; explicit tolerances for lower-priority objectives.

**Limits.** Weighted scores require disclosed units and weights. A weighted sum may miss nondominated points in a non-convex/discrete problem. Legal/accounting constraints are not soft objectives. A cheaper plan that endangers payroll is not allowed to win by a hidden score.

**Evidence.** Multi-objective cash-policy research and official optimization documentation support explicit competing goals. No particular solver vendor is required. [R11](#r11), [R33](#r33)

### M20 — Value of waiting and value of information

A decision to wait can preserve flexibility while incurring rent, lost use, price changes or expiring offers. Model those as dated cash/utility consequences in a scenario tree rather than an arbitrary waiting bonus.

For cost minimization with approved probabilities:

$$
EVPI=\min_x\sum_s p_sC(x,s)-\sum_s p_s\min_{x_s}C(x_s,s).
$$

This is the expected value of **perfect** information under the stated feasible sets. Information that is partial, late or costly has a different value and requires its own observation model.

**Proposed features.** Compare buying now, waiting for a client receipt, paying a refundable deposit, reserving a price, or obtaining a binding financing quote. Account for cancellation fees and what becomes known before commitment.

**Limits.** EVPI is an upper benchmark, not a promise of recoverable savings. A household car is not a freely traded option; do not import risk-neutral option pricing without its market assumptions. [R15](#r15)

### M21 — Goal funding, liability schedules and dynamic reserves

For goal $g$, contribution $u_{g,t}$ and required dated expenditure $L_{g,t}$:

$$
F_{g,t}=F_{g,t^-}+u_{g,t}+\text{credited growth}_{g,t}-L_{g,t}.
$$

If goals share actual accounts, these are allocations of the same underlying cash, not extra assets. Constraints must prevent total allocated funds from exceeding eligible funds.

**Proposed features.** Tuition schedules, wedding stages, car deposits, tax sinking funds, replacement funds, payroll buffers, priority changes and marginal goal-delay costs. Reserves can be derived from specified adverse obligation paths rather than an arbitrary fixed number of average-expense months.

**Limits.** “Six months of expenses” is a user policy, not a universal mathematical optimum. Committed bills, timing mismatches, company access restrictions and insurance payout delays can produce very different reserve needs. Goal shortfall penalties and priorities are user choices. Cash-flow matching provides a relevant mathematical foundation. [R12](#r12)

### M22 — Sensitivity, marginal value and explainable infeasibility

For a differentiable continuous optimization value $V(r)$, a local multiplier may estimate:

$$
V(r+\Delta r)-V(r)\approx \lambda\Delta r,
$$

with the sign defined by the constraint convention.

**Proposed features.** Explain the marginal cost of a higher emergency reserve, the value of an extra borrowing unit or the cost of bringing a purchase forward. For discrete taxes and account choices, re-solve finite changes rather than reporting a misleading derivative.

When constraints conflict, provide a named infeasible subsystem and alternatives that relax **user-soft** assumptions only. “Payroll reserve plus immediate car deposit exceeds legally accessible funds” is more useful than “no solution.”

**Limits.** An irreducible infeasible subsystem is not necessarily the unique or smallest conflict. Dual values are local and generally do not retain that interpretation for arbitrary integer models. Numerical diagnostics and feasibility relaxation require careful interpretation. [R28](#r28), [R29](#r29)

## 36. Tax and Lawful Extraction Mathematics

### M23 — Effective-dated tax function, not a flat rate

For a supported marginal-bracket schedule on taxable base $y$:

$$
T_{brackets}(y)=\sum_k r_k\min\{[y-l_k]_+,u_k-l_k\}.
$$

The top bracket can have an unbounded upper end, handled explicitly. The full tax function is a rule dependency graph:

$$
T=\mathcal T(\text{income types},\text{deductions},\text{credits},\text{status},\text{basis},\text{carryforwards};\text{jurisdiction, year, rule version}).
$$

**Proposed features.** Progressive schedules; thresholds and cliffs; surcharges; refundable/non-refundable credits; different tax bases; loss carryforwards with expiry; withholding status; contribution limits; employment benefits; source-country and residence-country rules; VAT/sales-tax obligations where configured.

**Limits.** The bracket equation does not handle all taxes by itself. Distinguish marginal, average and incremental rates. Rule precedence, exemptions, caps, rounding and filing units must match the actual local rules. IAS 12 is a reporting reference, not a source of tax rates. [R20](#r20)

### M24 — Liability, withholding, tax reserves and payment timing

A simplified reconciliation for one compatible assessment is:

$$
\text{balance payable}=T_{assessed}-\text{creditable withholding}-\text{credited advance payments}.
$$

Here $T_{assessed}$ already includes all assessment-stage deductions and tax credits exactly once, but not the withholding/advance-payment credits reconciled in this equation. A negative assessment balance may be a receivable, not immediate cash. Not all withholding is creditable; some may be final or subject to limitations.

**Proposed features.** Tax accrual state; remittance schedules; estimated-tax installments; withholding certificates; pending refunds; settlement matching; reserve creation/release; late-payment interest where lawfully configured; amended-assessment scenarios.

**Tests.** Withholding paid at source reduces cash once. At final assessment, apply its allowed credit rather than charging the same tax again. Release a reserve when its obligation is paid or revised. Show gross income, net receipt and tax separately.

**Terminology.** Tax payable later is not automatically an accounting “deferred tax liability”; temporary-difference accounting is a separate concept. [R20](#r20)

### M25 — Gross-up to meet an actual net funding requirement

Let $g$ be gross withdrawal and $N(g)$ immediately usable net proceeds:

$$
N(g)=g-W_{cash}(g)-F_{cash}(g),
\qquad \min g\ \text{subject to}\ N(g)\ge q.
$$

Future incremental tax is modeled separately, including its reserve and due date. For several sources, optimize gross amounts jointly under account and legal constraints.

**Proposed features.** “How much must I extract to receive 1m net?”; threshold-aware splitting where lawful; bank-route comparisons; fee-inclusive cost; marginal effective extraction cost; funding account residuals.

**Limits.** The shortcut $g=q/(1-r)$ is valid only for its flat-rate, no-other-cost special case. Some threshold rules make net proceeds non-monotone, so generic binary search can fail. Per-day aggregation and anti-avoidance rules must be enforced; the optimizer must not manufacture savings by splitting a transaction contrary to the configured law. The need for joint tax-aware withdrawal analysis is supported by personal-finance research. [R06](#r06), [R08](#r08)

### M26 — Multi-year tax-aware funding optimization

A possible cost objective is:

$$
\min_x\sum_{t=0}^{H}D_t\bigl(T^{cash}_t(x)+Fees_t(x)+FinancingCost_t(x)\bigr)
+D_H\,TerminalAdjustment_H(x),
$$

subject to required net spending, legal actions, reserves and obligations. Alternatively maximize terminal after-tax resources under the same consumption and risk requirements. Avoid adding both realized cash tax and the same accrued tax liability to an objective twice.

**Proposed features.** Compare withdrawal sequences across years, contribution/withdrawal timing, permitted account conversions, bracket use, carryforward expiry and owner remuneration timing. Report immediate cash tax, final liability and multi-year incremental tax separately.

**Limits.** The terminal adjustment must prevent the optimizer from hiding unpaid tax or debt just beyond the horizon. A low first-year tax plan can have a higher lifetime cost. Future law changes are named scenarios, not facts. The research supports multi-period tax planning, not one universally optimal withdrawal order. [R06](#r06), [R08](#r08)

### M27 — Company-to-person extraction with legal capacity

Define allowed methods $m$ such as salary, permitted dividends, documented reimbursement or repayment of a genuine shareholder loan. The optimization chooses $x_{e,p,m,t}$ only where eligibility and evidence are satisfied.

$$
\text{net household receipt}=\sum_{e,p,m,t}\operatorname{NetReceipt}(x_{e,p,m,t}),
$$

subject to company payroll, working capital, taxes, debt covenants, distribution authority and other applicable constraints.

**Proposed features.** Joint employer/employee/owner/company tax comparison; owner's two-company funding strategy; employee payroll protection; minority-owner distributions; extraction approval dates; retained-profit constraints; reimbursable expense matching; shareholder-loan balances.

**Limits.** Book cash does not prove lawful distributable profit. A personal withdrawal cannot be relabeled a loan or reimbursement merely to reduce tax. The permitted-action set requires jurisdiction-specific legal review and documentation; unknown eligibility prevents a “tax-optimal” conclusion. Household consolidation must not erase entity-level obligations. This is a proposed extension of tax-aware personal funding, with entity/reporting distinctions informed by [R06](#r06), [R20](#r20), [R21](#r21).

### M28 — Tax-lot-aware investment liquidation

For sales of quantity $q_\ell$ from tax lot $\ell$:

$$
G=\sum_\ell q_\ell(P_\ell-Basis_\ell),\qquad
0\le q_\ell\le Holding_\ell.
$$

Actual tax depends on holding period, permitted loss offsets, currency basis, jurisdiction, timing and account wrapper. Adjusted basis is not necessarily the original purchase price.

**Proposed features.** Fund a purchase by comparing lots, after-tax proceeds and portfolio risk; preserve lot records through corporate actions; model realized gains/losses; compare sell-versus-borrow alternatives; model loss restrictions only where supported by verified rules.

**Limits.** A “lowest gain first” heuristic can conflict with diversification or future tax costs. The cited research contains convex constructions and approximations; its performance is not proof that every real tax-lot problem is convex or globally solved. Account for real transaction costs even if a research formulation simplifies them. [R07](#r07)

### M29 — Cross-jurisdiction, threshold and legal-rule uncertainty

$$
\Delta T_j=T_j(\text{action scenario})-T_j(\text{matched baseline})
$$

for each jurisdiction/entity/assessment, followed by allowed credit/netting rules rather than automatically summing or canceling everything.

**Proposed features.** Residency changes; foreign income and remittances; source withholding; treaty/credit limits; filing-unit changes; company ownership changes; tax status transitions; effective dates; tax-rule version comparison; unknown-law scenarios; official-source provenance.

**Limits.** No jurisdiction is inferred from user timezone. No rates in illustrative examples are legal advice. Optimization is restricted to verified eligible actions; a source citation alone is not a rule-pack implementation test. Cross-border credits and group loss relief must not be presumed available. Unverified tax coverage must downgrade the result to an estimate and explicitly state omitted rules. The tax-state distinction follows [R20](#r20); all local substantive rules require separate official sources before implementation.

## 37. Loans, Cards and Financing Mathematics

### M30 — Contract-level amortization

For each actual accrual/payment interval:

$$
B_t^{debt}=B_{t^-}^{debt}+Draw_t+CapitalizedInterest_t+CapitalizedFees_t-PrincipalPaid_t.
$$

Track cash-paid interest and fees separately so they do not reduce principal or become capitalized twice. Payment allocation follows the contract.

The regular fixed-rate, equal-period, fully amortizing special case is:

$$
A=P\frac{r}{1-(1+r)^{-n}},\qquad A=P/n\text{ when }r=0.
$$

**Proposed features.** Floating/reset rates, irregular dates, grace periods, interest-only phases, balloon payments, final true-ups, negative amortization, installment holidays, late charges, refinancing, partial disbursement and prepayment penalties.

**Limits.** The annuity formula remains useful as a verified special case; it does not override contract day-count, fees or rounding. Published regulatory APR methodology demonstrates why timing and cash-flow conventions must be explicit, but local loan contracts govern the actual schedule. [R23](#r23)

### M31 — Effective rates, dated IRR and transparent borrowing cost

For a nominal annual rate $j$ compounded $m$ times annually:

$$
i_{eff}=(1+j/m)^m-1.
$$

For dated cash flows and a declared year fraction $\tau_i$:

$$
NPV(r)=\sum_i\frac{CF_i}{(1+r)^{\tau_i}},\qquad NPV(r)=0\text{ defines an IRR root}.
$$

**Proposed features.** Fee-inclusive economic borrowing rates, rate-convention conversion, irregular-flow return metrics, all-roots detection where practical, numerical brackets and residuals, fallback to NPV comparisons when IRR is ambiguous.

**Limits.** IRR can have multiple roots or no economically useful root. A generic XIRR is not automatically a legally disclosed APR. Regulatory APR conventions are jurisdiction- and product-specific. The user must be able to see every included and excluded fee and its date. [R23](#r23)

### M32 — Debt payoff and refinancing under liquidity constraints

Compare baseline and alternative dated cash flows:

$$
\Delta NPV=\sum_t D_t(CF_t^{alternative}-CF_t^{baseline}).
$$

Optimize repayment amounts subject to current/future reserves, minimum payments, penalties, tax treatment, rate resets and credit availability.

**Proposed features.** Avalanche versus other repayment preferences; earliest debt-free date; interest saved; liquidity sacrificed; refinance break-even date; payment reduction versus total-cost increase; term reset; redraw/recast restrictions; debt-consolidation comparisons.

**Limits.** Paying the highest stated rate first is optimal only under restrictive assumptions about fees, tax, compounding, available cash and contract rules. A lower monthly payment may result from a longer term and higher lifetime cost. Compare equivalent horizons and include remaining debt at the boundary. Lifecycle-cost methodology and cash-flow valuation support this comparison, with contract-specific finance rules required separately. [R17](#r17), [R23](#r23)

### M33 — Credit-card buckets, grace and statement timing

Where a card contract uses daily compounding for bucket $k$, an illustrative state is:

$$
B_{k,d+1}=(B_{k,d}+EligibleNetActivity_{k,d})(1+r_{k,d}),
$$

but the engine must implement the actual average-daily-balance/accrual and rounding convention, not assume this form universally.

**Proposed features.** Purchases, cash advances and balance transfers with separate rates; grace eligibility; promotional expiries; minimum-payment rules; payment-allocation order; statement date versus due date; installment conversion; card taxes; annual fees; rewards eligibility, caps and redemption value.

**Limits.** Paying a card is liability settlement, not a second purchase expense. Available credit is not current cash. Rewards count only under explicit earning and redemption assumptions; an unredeemable point balance is not spendable income. CFPB materials provide a US reference for grace/interest concepts, not universal card law. [R24](#r24)

### M34 — Alternative financing and economically equivalent comparisons

Represent financing by its enforceable cash-flow and ownership obligations rather than forcing every product into an interest-rate template:

$$
Cost_{economic}=PV(\text{all required payments and fees})-PV(\text{net funding or asset value received}),
$$

using a matched valuation basis and including terminal obligations.

**Proposed features.** Hire purchase, leases, balloon financing, seller installments, employee loans, family loans, zero-interest promotions, fee-based products, profit-sharing arrangements and user-supplied contractual structures. Model ownership transfer, deposits, residual guarantees, early termination and contingent payments.

**Limits.** A generic cash-flow comparison does not certify legal, accounting, religious or regulatory compliance. Profit-sharing cannot be treated as a fixed debt payment if the contract makes it conditional. Compare both total economic cost and date-specific cash feasibility. Methodological basis: lifecycle costing and dated finance cash flows. [R17](#r17), [R23](#r23)

## 38. Valuation, Purchases, Assets and Investments

### M35 — Discounting and real purchasing power

With effective interval discount rates $d_k$:

$$
D_t=\prod_{k\le t}(1+d_k)^{-1},\qquad PV=\sum_t D_tCF_t.
$$

For matching nominal interest and inflation intervals:

$$
1+r_{real}=\frac{1+r_{nominal}}{1+\pi}.
$$

**Proposed features.** Nominal versus real goals; separate inflation assumptions for rent, education, medical costs and vehicle expenses; term structures; sensitivity to discount rates; inflation-linked income/debt contracts; expected purchasing power.

**Limits.** Do not inflate a cash flow twice or discount a real cash flow at an inconsistent nominal rate. A discount rate is an economic assumption, not a guaranteed investment return. A high-NPV decision can still be unaffordable because bills arrive before income. Lifecycle-cost methodology explicitly motivates consistent dated-cost comparisons. [R17](#r17)

### M36 — Full after-tax total cost of ownership

For a financed asset, sum actual signed flows, not overlapping labels:

$$
TCO=PV(\text{purchase price}-\text{loan proceeds}+\text{loan payments}
+\text{operations}+\text{taxes/fees}-\text{resale}+\text{terminal debt settlement}).
$$

The terminal settlement is included only when not already present in payments and when the comparison closes the debt position. A cash purchase has no financing flows.

**Proposed features.** Car price/down-payment/term comparisons; fuel, repairs, insurance and registration; price uncertainty; maintenance shocks; resale timing/range; financing versus cash; cost per ownership year or use unit when well defined.

**Tests.** An 8m car funded by 3m cash and 5m borrowing creates 3m initial net cash use, not 8m plus 3m. Future repayments are separate. Identical economics must agree whether represented by gross purchase/loan flows or a correctly netted initial funding flow.

**Limits.** Salvage and repair values remain assumptions; quoted monthly payments alone do not establish total cost. [R17](#r17)

### M37 — Company projects, capital expenditure and break-even

$$
NPV_{project}=\sum_t D_t\Delta CF_t,
$$

where incremental cash flows include taxes, working capital, capex, maintenance and terminal recovery relative to a matched no-project baseline.

For a constant-price, constant-unit-variable-cost, fixed-cost special case:

$$
Q_{break-even}=\frac{FixedCost}{UnitPrice-UnitVariableCost},
$$

provided the contribution margin is positive.

**Proposed features.** Hire an employee, buy machinery, open a second location, increase inventory, lease equipment or invest retained cash; estimate capital required before break-even and the household income implications. Support capacity limits, stepped hiring costs and uncertain sales paths.

**Limits.** Accounting profit is not cash. The simple break-even formula does not handle stepped costs, working-capital delays or nonlinear pricing; use the full scenario model. Positive NPV does not prove the company can meet payroll during the investment. [R17](#r17), [R19](#r19)

### M38 — Rent, buy, lease and replacement decisions

Compare matched streams of housing/asset use:

$$
\Delta PV=PV(Cost_{buy})-PV(Cost_{rent/lease}),
$$

alongside minimum cash, debt, tax and terminal-asset outcomes.

**Proposed features.** Home down payment, property costs, rent escalations, maintenance, deposits and their return delays; vehicle replacement; old-asset sale before/after purchase; temporary double rent; bridge financing; transaction taxes and selling costs; holding-period breakpoints.

**Limits.** Include retained savings/investment assumptions consistently; do not count hypothetical investment returns as certain. Match property/use quality or explicitly record the difference. Buying and renting can differ in flexibility and risk; monetary equivalence does not decide user preferences. Research on waiting and lifecycle costing supplies the method, not a universal rent-versus-buy answer. [R15](#r15), [R17](#r17)

### M39 — Liability-driven saving and cash-flow matching

For holdings $q_j$ with dated instrument receipts $CF_{j,t}$:

$$
b_t=b_{t^-}+\sum_j CF_{j,t}q_j-L_t,
\qquad b_t\ge R_t,
$$

with purchase costs, taxes, eligibility and settlement constraints. Instruments can include deposits or bonds when appropriate to the user's actual access and risk assumptions.

**Proposed features.** Match school fees, tax dates, payroll buffers or retirement obligations with maturities; optimize ladder purchase costs; compare reinvestment risk; stress default, early-withdrawal penalty and inaccessible funds.

**Limits.** Equal present values do not guarantee cash arrives before each bill. Duration matching is a sensitivity approximation, not complete cash-flow matching. A contractual coupon is subject to issuer/default and settlement assumptions. Published work explicitly combines cash-flow matching with tail-risk controls. [R12](#r12)

### M40 — Portfolio risk, allocation and performance attribution

For user-approved expected returns $\mu$, covariance $\Sigma$ and weights $w$:

$$
\mu_p=w^T\mu,\qquad \sigma_p^2=w^T\Sigma w.
$$

**Proposed features.** Optional asset-allocation analysis; concentration by company/employer/currency; after-tax rebalancing; liquidity-aware target allocations; risk-budget trade-offs; money-weighted versus flow-adjusted time-weighted performance; scenario drawdowns; portfolio funding of household goals.

**Limits.** Mean/variance inputs are assumptions, not predictions. Covariance must be valid and sensitivity-tested. Normality is not implied. Investment risk must include salary/employer exposure and company ownership where modeled; short historical returns cannot establish reliable long-term tail probabilities. Tax lots, fees, settlement and liabilities remain constraints.

**Evidence.** Multi-period investment and tax-aware portfolio research provide relevant formulations. Their optimizer does not supply guaranteed return forecasts, and paper-specific convex approximations must be labeled. [R05](#r05), [R07](#r07)

## 39. Company Operations, Receivables and Treasury

### M41 — Payroll as linked gross, net and remittance flows

For a configured employee payment:

$$
GrossPay=NetPay+EmployeeWithholding+EmployeeContributions,
$$

$$
EmployerCost=GrossPay+EmployerContributions+EmployerBenefits+OtherEmployerCosts.
$$

Adjust the categories to the jurisdiction and contract; non-cash benefits need separate treatment.

**Proposed features.** Multiple employee contracts, raises, leave, bonuses, commissions, final pay, employer taxes, withholding remittance, pension contributions, expense reimbursement, arrears and partial payment. Show company cash timing separately from accrued labor expense.

**Household link.** An owner's net payroll receipt can feed their household forecast. Other employees' wages remain external company outflows. The owner salary must not be added to the household while omitted from the company or counted twice in a combined view.

**Limits.** Detailed payroll law requires verified local rules. Gross/net arithmetic alone does not establish legal pay, leave or employer liabilities. Cash/non-cash and tax-state distinctions follow [R19](#r19), [R20](#r20).

### M42 — Working capital and cash conversion

A conventional diagnostic is:

$$
CCC=DSO+DIO-DPO,
$$

where the periods, averages and denominators for receivable, inventory and payable days must be explicitly defined. The operational forecast uses actual invoice and settlement schedules, not this ratio as a substitute.

**Proposed features.** Accounts receivable/payable aging; inventory purchases and sale timing; customer deposits; supplier prepayments; taxes collected for remittance; payroll dates; minimum working capital; customer/supplier concentration; factoring and supplier-discount comparisons; staged project billing.

**Limits.** Undefined ratios, negative margins and seasonal distortions require warnings rather than arbitrary zero values. A profitable company may face a payroll shortage while waiting for customers to pay. Factoring is funding with recourse/fees/eligibility as applicable, not new revenue. Cash-flow-management research provides the stronger dated funding foundation. [R09](#r09), [R19](#r19)

### M43 — Receivable timing, partial collection and censored evidence

With suitable collection-time data, an optional descriptive survival estimate is:

$$
\widehat S(t)=\prod_{t_j\le t}\left(1-\frac{d_j}{n_j}\right),
$$

where $d_j$ is the number of qualifying collection events and $n_j$ the number still at risk just before $t_j$. This is the Kaplan–Meier construction.

**Proposed features.** Include still-outstanding invoices in delay analysis; user-approved collection-date assumptions; partial-settlement schedules; default/write-off states; dispute pauses; recovery costs; invoice cohorts and shared-customer shocks. Optionally compute discounted expected cash shortfall under explicit approved probabilities.

**Limits.** “Fully collected invoice” and “first partial payment” are different events. Write-offs/defaults are not automatically harmless censoring; competing events and informative censoring can invalidate a simple estimate. Small or changing client samples do not support confident prediction. An expected credit loss/provision is not an extra cash payment and must not be counted again when cash fails to arrive. The survival method comes from [R18](#r18); financial-instrument reporting concepts are distinct under [R22](#r22).

### M44 — Cash sweep policies and classical benchmarks

For uniform predictable cash use $T$ per period, fixed transfer cost $K$, and opportunity rate $i>0$ per matching period, minimize:

$$
Cost(C)=K\frac{T}{C}+i\frac C2,
\qquad C^*=\sqrt{\frac{2KT}{i}}.
$$

This is the classical inventory-style cash-transfer benchmark.

Under the classical Miller–Orr conditions, with daily net-flow variance $\sigma^2$, compatible daily opportunity rate $i$, fixed cost $K$, and selected lower limit $L$:

$$
d=\left(\frac{3K\sigma^2}{4i}\right)^{1/3},\qquad ReturnPoint=L+d,\qquad UpperLimit=L+3d.
$$

**Proposed features.** Compare manual sweep rules, transfer frequency, idle cash cost and operational buffers. Use these formulas as benchmarks for a one-pool idealization.

**Limits.** Do not use either formula as the primary planner for scheduled salaries, threshold taxes, multiple entities or delayed transfers. Miller–Orr's classical stochastic-flow assumptions are not automatically valid for payroll. The general network model must prevail when assumptions fail. [R13](#r13), [R14](#r14)

### M45 — Contingent liabilities and shared-cause liquidity stress

For contingent obligation $j$:

$$
Payment_{j,t}(s)=Trigger_j(s)\times Amount_{j,t}(s),
$$

with a contract-specific trigger, cap, sequence, recourse and payment date.

**Proposed features.** Personal guarantees of company debt, cross-default clauses, supplier deposits, legal settlements, customer refunds, partner exits, shareholder capital calls, bank outages, credit-line withdrawal, key-client default and temporary account freezes. Link a common trigger across every affected company and household income stream.

**Limits.** Contingent debt is not automatically paid today, but it must not be omitted from stress planning. A credit line can be committed, conditional or cancelable; it is not equivalent to cash already held. Regulatory bank stress/liquidity principles are inspiration for scenario completeness, not household regulatory requirements. [R25](#r25), [R26](#r26)

## 40. Lifetime, Insurance and Shared-Finance Extensions

### M46 — Retirement and longevity-conditioned cash flows

For an approved mortality table with one-period death probabilities $q_{x+k}$:

$$
{}_t p_x=\prod_{k=0}^{t-1}(1-q_{x+k}),
\qquad EPV=\sum_t D_t\,{}_t p_x\,Payment_t.
$$

Also test fixed longevity horizons and joint-life scenarios rather than relying only on expected values.

**Proposed features.** Retirement-date choices, pension start dates, contribution limits, inflation-linked benefits, mandatory withdrawals where applicable, survivor benefits, care costs, bequests and flexible consumption. Re-plan with actual assets and changed assumptions.

**Limits.** A population life table is not a personal lifespan forecast. Joint-life dependence, health-related costs and benefit eligibility require additional inputs. Expected present value does not establish that money lasts in a long-life scenario. The retirement MPC paper supplies a relevant planning architecture; SSA tables are an example of an official actuarial input source, not a prescribed jurisdiction or population. [R06](#r06), [R31](#r31)

### M47 — Insurance deductibles, retention and payout delay

For a simple contract with loss $L$, deductible $d$ and insurer-payment cap $u$:

$$
InsurerPayment=\min\{[L-d]_+,u\},\qquad RetainedLoss=L-InsurerPayment.
$$

More complex contracts add coinsurance, exclusions, waiting periods, aggregate limits, reinstatement and benefit rules.

**Proposed features.** Compare premium versus retained tail loss; household emergency funding before reimbursement; business interruption; disability/lost salary; asset damage; healthcare out-of-pocket limits; underinsurance; claim denial or delay scenarios.

**Limits.** The wording of the actual policy controls; “limit” may mean different things in different contracts. An expected claim is not available cash before settlement. Selecting the lowest expected cost alone can create unacceptable tail exposure. Apply explicit worst-case or CVaR loss constraints when their input conditions hold. Risk-metric foundation: [R02](#r02), [R03](#r03).

### M48 — Shared-cost allocation and settlement

Support simple user-selected policies first: equal amounts, income proportion, ownership percentage, explicit shares or reimbursement agreements. For an optional cooperative allocation of a defined coalition cost/value $v$:

$$
\phi_i(v)=\sum_{S\subseteq N\setminus\{i\}}
\frac{|S|!(n-|S|-1)!}{n!}[v(S\cup\{i\})-v(S)].
$$

**Proposed features.** Explain contributions to shared expenses or jointly achieved savings; compare allocation policies; net inter-person balances; minimize the number/cost of permitted settlement transfers.

**Limits.** The Shapley value satisfies particular mathematical allocation axioms; it is not proof of moral fairness. The coalition function must be defined and accepted by the people involved. Exact enumeration grows rapidly; any approximation needs an explicit method and replayable sample. Cost-sharing does not change the original merchant expense or create new household income. [R16](#r16)

### M49 — Estate, succession and administrative liquidity scenarios

$$
Available_{a,t}(s)=AccessIndicator_{a,t}(s)\times EligibleBalance_{a,t}(s),
$$

with separate tax, ownership-transition and benefit events. The binary indicator is a simplified representation; partial access and staged releases may be needed.

**Proposed features.** Death/incapacity scenarios, temporary account inaccessibility, survivor income, company succession, buy-sell obligations, estate expenses, beneficiary distributions, insurance payouts and liquidity needed before asset transfer. Include disability, divorce or household membership changes as optional scenario events without designing interpersonal privacy now.

**Limits.** No automatic inheritance share, probate duration, tax treatment or beneficiary entitlement is assumed. Those require verified local law and actual documents. Mathematical planning here identifies dated funding needs, not legal advice or document generation. Longevity and contingent-liquidity concepts motivate the module. [R26](#r26), [R31](#r31)

### M50 — Sequence risk and flexible spending policies

For a portfolio pool, one simplified timing convention is:

$$
W_{t+1}=(W_t-C_t)(1+r_{t+1}),
$$

where consumption occurs before the period's return. Another timing convention produces a different equation and must be declared.

**Proposed features.** Compare different return sequences with the same compound growth; flexible retirement spending; delayed discretionary purchases; minimum essential consumption; reserve-triggered spending reductions; income replacement during job gaps. Add job/business income factors to the same scenario paths.

**Limits.** Averaging investment returns loses withdrawal-sequence effects. A fixed withdrawal percentage is a policy candidate, not a guarantee. Guardrails must be user-defined, legally executable and tested for essential-spending shortfalls. Receding-horizon and risk-sensitive planning are appropriate foundations, with returns remaining assumptions. [R05](#r05), [R06](#r06), [R32](#r32)

## 41. Solver, Rule-Engine and Guarantee Requirements

### M51 — Match the mathematical method to the actual model

$$
\min_x f(x)\quad\text{subject to }g(x)\le0,\ h(x)=0,\ x_I\in\mathbb Z.
$$

**Proposed solver profiles.** Exact arithmetic event evaluation; network/min-cost flow; linear programming; convex quadratic/conic programs; mixed-integer linear/convex models; supported dynamic programming; finite scenario enumeration; deterministic parameter grids; globally bounded nonlinear methods where available; explicitly labeled heuristics otherwise.

**Requirements.** Document why a model belongs to a class before applying a theorem. Tax thresholds, fixed fees, indivisible purchases and employment states can introduce integers/non-convexity. User rules can destroy monotonicity or convexity. Large finite grids are not continuous optimization.

**Limits.** A convex planning result from a paper does not mean arbitrary tax code is convex. Include unit scaling, bounded variables, tight indicator formulations where supported and exact contract replay. [R07](#r07), [R28](#r28)

### M52 — Feasibility, optimality and approximation certificates

For a minimization problem with feasible incumbent $UB$ and valid lower bound $LB$:

$$
0\le UB-f^*\le UB-LB.
$$

Report an absolute bound in objective units. Any normalized gap must state its denominator and handle zero/negative objectives sensibly; do not silently substitute it for the solver's own definition.

**Required outputs.** Feasible/infeasible/unbounded/unknown status; primal residuals; integer tolerance; objective bound; gap; model class; scenario coverage; search domain; time/work limit; numerical warnings; approximation status; binding constraints; exact replay outcome.

**Limits.** Floating-point solver status is not automatically an exact mathematical proof. A heuristic may find a useful feasible answer without a global bound. If a robust adversary is unresolved, do not claim universal feasibility. Exact or independently checked certificates can be supported where practical. [R28](#r28), [R29](#r29)

### M53 — Reproducible execution and penny-level validation

$$
Result=Calculate(\text{immutable inputs},\text{rules},\text{algorithm},\text{solver configuration},\text{scenario sample}).
$$

**Requirements.** Stable model ordering, deterministic tie-breaking, preserved seed/sample, deterministic solver configuration where available, software/version metadata, calendar/FX snapshots and declared rounding. Use deterministic work budgets where supported rather than claiming identical output from a wall-clock cutoff. Record when reproducibility is platform-specific.

After optimization, replay all proposed actions through the authoritative decimal/contract engine. Verify actual net proceeds, legal eligibility, funding dates, account floors, payroll and taxes. A feasible floating solution that fails penny-level replay is not an executable plan. Re-solve with appropriate monetary granularity, rounding-aware constraints or a disclosed corrective search; do not quietly change amounts while retaining the original optimality claim.

**Limits.** Same seed alone does not guarantee identical results across solver versions, hardware, parallel algorithms or changed model ordering. Official solver guidance documents these distinctions. No specific vendor is mandated. [R28](#r28), [R30](#r30)

### M54 — Auditable user rules with declared mathematical properties

A rule must expose:

$$
Output=Rule(Input;Scope,EffectiveDates,Version,Rounding,Priority).
$$

**Proposed features.** Typed variables and currency units; thresholds/caps/brackets; lookups and calendars; conditional triggers; aggregation windows; net/gross tax basis; dependency graph; example tests; conflict resolution; cycle detection; safe expression evaluation; provenance and change comparison.

The engine should classify each supported expression as affine, piecewise-linear, monotone, convex, discrete, nonlinear or unknown **using deterministic structural rules**, not AI. A class label permits an optimization shortcut only when its conditions are verified. Arbitrary unverified code cannot be treated as a proven monotone tax function.

**Limits.** User overrides can simulate alternatives, but they must not silently replace a verified legal rule in a result labeled legally compliant. Unsupported rules can still be evaluated in a scenario without claiming an optimizer guarantee. Numerical formulation and validation concerns are grounded in [R28](#r28); the rule-engine contract is a proposed engineering requirement.


### M55 — Authorization-aware aggregation and provenance projection

Let a calculation context be $c$, a viewer be $v$, and each financial object be $i$. Define deterministic policy functions:

$$
A_i(c) \in \{0,1\}
$$

for whether object $i$ is authorized to participate in calculation context $c$, and

$$
D_i(v,c) \in \{\text{hidden},\text{aggregate},\text{balance-only},\text{selected-fields},\text{full}\}
$$

for the permitted disclosure level to viewer $v$ in that context.

If the underlying calculation is

$$
Y(c)=f(\{x_i:A_i(c)=1\}),
$$

the authoritative result uses all and only authorized calculation inputs. The explanation returned to viewer $v$ is a deterministic projection of the full calculation graph:

$$
E_v(c)=Project(G(c),D(v,c)),
$$

where $G(c)$ is the complete provenance graph. `Project` may redact labels/values, replace multiple restricted nodes with an authorized aggregate, or suppress a derivation that would violate policy. It must not alter the authoritative result merely to make an explanation convenient.

**Required invariants.**

1. Economic ownership and accounting entries are unchanged by visibility settings.
2. Unauthorized objects cannot enter a calculation merely because another viewer can see their aggregate.
3. Authorized-but-restricted objects can participate without exposing restricted node-level provenance.
4. A viewer with fuller authorization receives a refinement of the explanation, not a numerically different result for the same calculation context unless their context itself authorizes additional inputs.
5. Exports, APIs, notifications, logs and scenario-difference reports apply the same policy projection.
6. Policy version and purpose/context are part of the calculation record.
7. When an aggregate or difference would trivially reconstruct a restricted value, the disclosure engine must apply the configured suppression/coarsening rule or require explicit authorization.

**Limits.** This is an application authorization model, not a cryptographic confidentiality proof. If mutually distrustful participants, compromised servers or cross-party computation become part of the threat model, techniques such as secure multi-party computation, homomorphic encryption, differential privacy or trusted execution environments would require separate research, performance evaluation and security design.

## 42. Required Data and Provenance Extensions

The earlier conceptual entities remain. Add the following to support the mathematical requirements:

| Data family | Required records |
|---|---|
| State | Currency-specific journal entries; settled/pending availability; debt principal/interest buckets; tax basis; accrued obligations; earmark coverage |
| Uncertainty | Assumption path; factor dependency; uncertainty set/version; budget groups; scenario-tree node; revelation date; user-approved distribution; optional replayable sample |
| Tax/legal | Jurisdiction/filing unit; rule-source record; verification status; tax credit/carryforward; withholding certificate; assessment/payment/refund; extraction eligibility/documentation |
| Contracts | Loan schedule; rate reset; card bucket; deposit terms; insurance terms; employee contract; supplier/customer terms; ownership/distribution constraints |
| Decisions | Locked commitment; reversible action; observation-dependent policy; objective hierarchy; soft constraint; funding route; Pareto alternative; terminal-value assumptions |
| Research | Method identifier; source version; mathematical assumptions; applicable model class; limitations; benchmark/test reference |
| Results | Input hash; calculation graph; native/reporting currency basis; solution status; residuals; bound/gap; scenario coverage; worst-case witness; exact replay; difference from prior result |
| Historical evaluation | Original forecast; actual outcome; reconciliation links; error/coverage metrics; training/evaluation split; data-source freshness |
| Authorization | Access policy/version; grant; role; permitted viewers; calculation purpose; disclosure level by field/result; policy effective dates; privacy-audit event; suppression/coarsening rule |

**Every derived result must expose** its units, valuation date, horizon, boundary, inputs, exclusions, applicable rules, assumptions, formula/algorithm and result-strength label. An explanation may be generated from deterministic templates and the calculation graph; it must not depend on an LLM.

For decision-dependent forecasts, identify which future actions are assumed and whether they are already committed. A future balance that silently assumes selling an asset is as misleading as one that silently assumes receiving salary.


<a id="feature-register"></a>
## 43. Research-Expanded Candidate Feature Register

Each entry is a requirement candidate for the full product inventory, not a delivery commitment. Overlapping models are referenced deliberately where a feature spans accounting, uncertainty and optimization. The earlier domain requirements remain in force.

### Financial state, ownership and timing

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F001 | Native-currency balanced journal with immutable reversals | M01 |
| F002 | Separate economic, posting, settlement and spendable dates | M01, M05 |
| F003 | Current cash excludes all expected future receipts | M01, M02 |
| F004 | Decision-specific accessible funding, not one fungible household sum | M02, M03 |
| F005 | Non-overlapping earmarks and nested minimum-balance constraints | M02, M21 |
| F006 | Personal, joint, company and third-party ownership shares | M04 |
| F007 | Company valuation versus underlying-asset consolidation modes | M04 |
| F008 | Eliminate internal flows without deleting standalone legal liabilities | M04, M41 |
| F009 | Contract-specific recurrence, holidays, leap years and month ends | M05 |
| F010 | Intraday ordering and account-specific automatic-payment failures | M05 |
| F011 | FX valuation separate from executable conversion and settlement | M06 |
| F012 | Stale balances, missing transactions and incomplete state warnings | M01, M15 |

### Uncertainty and explicit financial risk

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F013 | Joint salary, rent, employment and collection-date assumption paths | M07 |
| F014 | Shared-customer and shared-employer dependency factors | M07, M45 |
| F015 | Finite-scenario coverage labels distinct from robust guarantees | M08, M52 |
| F016 | Robust date/down-payment feasibility under a named uncertainty set | M08 |
| F017 | Budgeted uncertainty and cost of protecting against more joint shocks | M09 |
| F018 | Whole-horizon breach probability with approved probabilities only | M10 |
| F019 | CVaR of funding deficits, cost and goal shortfalls | M11 |
| F020 | Distributional-ambiguity sensitivity and supported DRO models | M12 |
| F021 | First breach, worst deficit, duration and minimum extra capital | M13 |
| F022 | Joint reverse-stress thresholds and smallest modeled failure shocks | M14 |
| F023 | Minimax-regret alternatives without invented probabilities | M14 |
| F024 | Frozen historical backtests, forecast error and interval coverage | M15 |

### Adaptive decisions and competing goals

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F025 | Scenario trees with information-revelation dates | M16 |
| F026 | Nonanticipative decisions shared across indistinguishable scenarios | M16 |
| F027 | Invoice-settlement and confirmed-funding purchase triggers | M16, M20 |
| F028 | Re-plan after actual outcomes while preserving locked commitments | M17 |
| F029 | Supported dynamic programming and time-consistent risk policies | M18 |
| F030 | Pareto alternatives across tax, cost, risk, delay and goals | M19 |
| F031 | Lexicographic priorities and explicitly permitted objective degradation | M19 |
| F032 | Waiting, deposits, cancellation fees and expiring offers | M20 |
| F033 | Value of information with explicit observation timing | M20 |
| F034 | Multi-stage goals with shared-account funding constraints | M21 |
| F035 | Local marginal values and finite-change sensitivity for discrete rules | M22 |
| F036 | Explain conflicting constraints and user-approved relaxations | M22, M52 |

### Taxes and lawful extraction

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F037 | Jurisdiction/year-specific tax dependency graphs | M23, M29 |
| F038 | Brackets, cliffs, credits, caps, deductions and carryforward expiry | M23 |
| F039 | Final tax liability separate from withholding and advance payments | M24 |
| F040 | Tax reserves, remittance dates, refunds and assessment reconciliation | M24 |
| F041 | Gross-up to a required net receipt after real taxes and fees | M25 |
| F042 | Cumulative thresholds and lawful aggregation rules across transactions | M25, M54 |
| F043 | Multi-year withdrawal and contribution timing optimization | M26 |
| F044 | Terminal tax/debt provisions to prevent horizon manipulation | M26, M52 |
| F045 | Company salary/dividend/reimbursement/genuine-loan alternatives | M27 |
| F046 | Employer plus employee plus company plus owner tax comparison | M27, M41 |
| F047 | Tax-lot liquidation and permitted realized-loss planning | M28 |
| F048 | Cross-border credits, residence changes and unknown-law scenarios | M29 |

### Banking, transfer paths and deposits

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F049 | Tax/fee/settlement-aware account routing | M03, M25 |
| F050 | Per-transaction, daily and rolling withdrawal/transfer caps | M03, M54 |
| F051 | Required prefunding before due dates and cutoffs | M03, M05 |
| F052 | Fixed and percentage fees, fee caps and minimum transaction sizes | M03 |
| F053 | Deposit maturities and early-withdrawal penalties | M03, M39 |
| F054 | Liquidity ladders aligned to household and company obligations | M39 |
| F055 | Cash-sweep recommendations with explicit operational buffers | M44 |
| F056 | Classical cash-policy benchmarks when assumptions hold | M44 |
| F057 | Bank outage, account freeze and unavailable transfer-route scenarios | M45 |
| F058 | Deposit concentration and configurable local protection/access limits | M06, M45 |
| F059 | Credit-line commitment, cancellation, draw timing and covenants | M30, M45 |
| F060 | Net inter-person settlements without illegal cross-entity netting | M03, M48 |

### Debt, credit cards and contractual finance

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F061 | Exact regular and irregular loan schedules | M30 |
| F062 | Floating-rate resets, grace, balloons and negative amortization | M30 |
| F063 | Net disbursement, upfront fees and capitalized cost treatment | M30, M31 |
| F064 | Effective-rate conversions and convention-specific APR support | M31 |
| F065 | Dated IRR root diagnostics and NPV fallback | M31 |
| F066 | Debt-payoff optimization constrained by cash and essential spending | M32 |
| F067 | Refinancing break-even including fees, term reset and terminal debt | M32 |
| F068 | Card purchase/cash-advance/promotion buckets and payment allocation | M33 |
| F069 | Statement/due-date/grace-period and minimum-payment calculation | M33 |
| F070 | Card taxes, foreign fees, rewards caps and redemption assumptions | M33 |
| F071 | Lease, hire purchase, family loan and seller installment comparison | M34 |
| F072 | User-defined nonstandard funding contracts without compliance claims | M34, M54 |

### Purchases, inflation and investments

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F073 | Nominal/real cash-flow consistency and category-specific inflation | M35 |
| F074 | Discount-rate and terminal-value sensitivity | M35, M36 |
| F075 | Complete after-tax vehicle/home/asset ownership cash flows | M36 |
| F076 | Cash versus financing without principal/down-payment double counting | M36 |
| F077 | Purchase, maintenance, insurance, resale and replacement uncertainty | M36, M38 |
| F078 | Rent/buy/lease with matched use and holding periods | M38 |
| F079 | Staged deposits, bridge funding and old-asset sale delays | M38 |
| F080 | Liability matching with maturity, tax and reinvestment risks | M39 |
| F081 | Optional asset allocation with explicit return/risk assumptions | M40 |
| F082 | After-tax rebalancing with lots, fees and settlement constraints | M28, M40 |
| F083 | Concentration across company equity, employer salary and investments | M07, M40 |
| F084 | Time-weighted versus money-weighted performance and scenario drawdowns | M31, M40 |

### Multi-company operations and payroll

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F085 | Multiple companies per person and multi-owner company constraints | M04, M27 |
| F086 | Employees, gross/net salary, benefits and employer cost | M41 |
| F087 | Payroll tax remittance, contribution dates and final-pay scenarios | M24, M41 |
| F088 | Company-to-household owner payroll linkage without double counting | M41 |
| F089 | Accrual profit versus dated cash availability | M37, M42 |
| F090 | Receivable/payable aging and partial settlement | M42, M43 |
| F091 | Inventory, customer deposits, supplier terms and cash conversion | M42 |
| F092 | Factoring, supplier discounts and early-payment comparisons | M31, M42 |
| F093 | Company working-capital and committed-payroll floors | M21, M27, M42 |
| F094 | Capex, hiring and expansion NPV plus funding-gap analysis | M37 |
| F095 | Shareholder loans, capital injections, distributions and partner exits | M27, M45 |
| F096 | Personal guarantees, cross-defaults and correlated company shocks | M45 |

### Insurance, lifetime and financial contingencies

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F097 | Retirement-date, pension-start and contribution/withdrawal scenarios | M46 |
| F098 | Fixed longevity horizons, official life tables and survivor cases | M46 |
| F099 | Sequence-of-returns risk with the same overall compound return | M50 |
| F100 | Flexible spending policies with protected essential consumption | M18, M50 |
| F101 | Deductibles, coverage limits, exclusions and retained losses | M47 |
| F102 | Insurance premium versus tail-risk and reserve trade-offs | M11, M47 |
| F103 | Claim delays and liquidity before reimbursement | M13, M47 |
| F104 | Disability, caregiving, medical shocks and interrupted business income | M45, M47 |
| F105 | Estate expenses, account lockup and succession liquidity | M49 |
| F106 | Beneficiary/survivor cash-flow schedules under verified eligibility | M46, M49 |
| F107 | Family membership, dependents and support-obligation change scenarios | M21, M49 |
| F108 | Bequest goals and terminal after-tax resources | M26, M46 |

### Shared funding, data and assumption governance

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F109 | Equal, income-proportional, ownership-based and custom cost sharing | M48 |
| F110 | Optional Shapley allocation for a defined coalition value | M48 |
| F111 | Payment responsibility separate from expense beneficiary/owner | M04, M48 |
| F112 | Inter-person receivables and reimbursement settlement | M01, M48 |
| F113 | Still-outstanding invoices included in collection-delay evidence | M43 |
| F114 | Default, dispute and partial-payment states distinct from censoring | M43 |
| F115 | Approved historical summary rules with sample/exclusion provenance | M15 |
| F116 | Unknown probability represented as unknown, not assigned zero | M10, M15 |
| F117 | Assumption expiry, source freshness and approval history | M07, M15 |
| F118 | Scenario source/version and immutable original forecast preservation | M15, M17 |
| F119 | Optional reproducible numerical integration under approved distributions | M10, M12, M53 |
| F120 | Import/reconciliation validation and duplicate-event prevention | M01, M53 |

### Optimization quality and calculation integrity

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F121 | Model-class detection before applying solver shortcuts | M51, M54 |
| F122 | Exact enumeration of supported finite candidate spaces | M51 |
| F123 | LP, convex, mixed-integer and supported dynamic-programming profiles | M18, M51 |
| F124 | Heuristic and discretization labels with exposed resolution | M51, M52 |
| F125 | Feasibility residuals, incumbent, bound and objective gap | M52 |
| F126 | Declared uncertainty-set coverage and worst-case witness | M08, M52 |
| F127 | Independent exact-decimal replay of proposed actions | M53 |
| F128 | Deterministic model ordering, solver configuration and tie-breaking | M53 |
| F129 | Stored seed/sample/version for any numerical simulation | M53 |
| F130 | User-soft versus legal/accounting hard constraint distinction | M22, M54 |
| F131 | Property-based tests, brute-force small-instance oracle comparisons | M51, M52, M53 |
| F132 | Every derived value has complete inputs, units, calculation and limitations internally, with viewer-authorized disclosure | M01–M55 |

### Adaptable rules and explainable decisions

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F133 | Typed user-defined financial expressions and unit checks | M54 |
| F134 | Versioned tax/card/withdrawal/bank-selection rules | M23, M54 |
| F135 | Effective dates, calendar versions and historical rule replay | M05, M54 |
| F136 | Rule dependency/cycle/conflict detection | M54 |
| F137 | Rule examples, threshold tests and cumulative-window tests | M54 |
| F138 | Verified official rules distinct from hypothetical user overrides | M29, M54 |
| F139 | Scenario difference attribution without hidden double counting | M17, M54 |
| F140 | Assumption-bound recommendation contract and objective disclosure | M16, M19, M52 |
| F141 | Current versus future action, reversibility and commitment disclosure | M16, M17 |
| F142 | Unsupported/missing rules prevent a false optimality claim | M29, M52 |
| F143 | All classifications, comparisons and explanations use inspectable logic | M53, M54 |
| F144 | No AI and no autonomous money movement; UX design deferred, privacy architecture not deferred | M01–M55 |

### Privacy, authorization and permission-aware provenance

| ID | Candidate requirement | Mathematical dependency |
|---|---|---|
| F145 | Object-level authorization boundaries independent of economic ownership | M04, M55 |
| F146 | Separate visibility of existence, balance, transactions, metadata, forecasts, assumptions and explanations | M55 |
| F147 | Calculation access independent from raw-data visibility | M55 |
| F148 | Private objects may be excluded, used as restricted contributions, or fully available by context | M55 |
| F149 | Minimum presets: Private, Shared summary, Shared balance, Fully shared | M55 |
| F150 | Selected-viewer and role-based grants for households and entities | M55 |
| F151 | Purpose-specific grants for household forecasts, named scenarios, goals and tax/funding analyses | M55 |
| F152 | Effective-dated, versioned access policies and immutable privacy audit events | M53, M55 |
| F153 | Viewer-authorized provenance projection from a complete internal calculation graph | M53, M55 |
| F154 | Restricted contributions can be shown as authorized aggregates without underlying source disclosure | M55 |
| F155 | Balance-visible/transaction-private account policy | M55 |
| F156 | Scenario, goal, decision and assumption privacy independent from account sharing | M16, M55 |
| F157 | Company roles and disclosure boundaries independent from household roles | M04, M41, M55 |
| F158 | Linked company-to-person events support different authorized representations on each side without double counting | M04, M41, M55 |
| F159 | Revocation affects new calculations while preserving immutable historical accounting and policy-versioned replay | M53, M55 |
| F160 | Export, API, notification and audit-output authorization parity with interactive calculations | M55 |
| F161 | Deterministic suppression/coarsening when differencing or decomposition would reconstruct restricted values | M55 |
| F162 | Privacy-policy conflicts, missing grants and unauthorized funding-source selection fail closed with explicit reason | M52, M54, M55 |


<a id="worked-examples"></a>
## 44. Worked Mathematical Examples

All examples below use hypothetical amounts and rules. They verify a particular calculation, not the legal or practical suitability of a financial strategy. The small numerical examples were independently checked during document preparation; they are not tests of an implemented application.

### E01 — Reserving money is not spending it

Start with 2,000,000 settled cash. Earmark 800,000 for emergencies, 300,000 for tax and 250,000 for school fees, all disjoint.

$$
Cash=2{,}000{,}000,\qquad Unreserved=2{,}000{,}000-800{,}000-300{,}000-250{,}000=650{,}000.
$$

Pay the 300,000 tax bill and release that earmark. Cash becomes 1,700,000; remaining earmarks total 1,050,000; unreserved cash remains 650,000. The same liability must not reduce spendability a second time after it has been paid.

**Relevant models:** M01, M02, M24.

### E02 — Gross withdrawal must actually deliver the requested net amount

Starting balances: Account A 1,500,000 and Account B 900,000. Assume a hypothetical strategy has 42,000 cash tax and 3,500 fees.

$$
Gross=645{,}500+400{,}000=1{,}045{,}500,
\qquad Net=1{,}045{,}500-42{,}000-3{,}500=1{,}000{,}000.
$$

Account A ends at 854,500 and Account B at 500,000. Withdrawing only 1,000,000 gross would deliver 954,500 net under these assumed costs, not 1,000,000. In production, the tax/fee function must be recomputed on the chosen gross amounts rather than assumed fixed without justification.

**Relevant models:** M03, M25.

### E03 — A dated, assumption-bound down-payment comparison

**Purpose:** Isolate the effect of payment timing and joint amount bounds. This is not a complete car affordability calculation: car financing, ownership costs, taxes and fees are deliberately excluded from this small fixture and must be included in a real purchase plan.

| Input | Explicit assumption |
|---|---|
| Opening settled cash | 2,000,000 on September 10, 2026, in one accessible account |
| Salaries | Each September 30, October 31 and November 30 receipt is between 480,000 and 520,000 |
| Employment | Those three salaries occur; there is no salary in December or January |
| Rent | October 1, November 1, December 1 and January 1: each between 180,000 and 200,000 |
| Other spending | October 15, November 15, December 15 and January 15: each between 120,000 and 140,000 |
| Client receipt | 300,000–400,000, arriving on one calendar day from November 10 through December 10 |
| Protected reserve | 1,000,000, throughout September 10, 2026–January 31, 2027 |
| Purchase ordering | Down payment occurs after other listed events on its date; a November 30 purchase follows salary settlement |
| Dependencies | Every combination of these specified amount/date bounds is allowed; no additional shocks or flows |

In this particular additive one-account model, cash is monotone in each income/expense amount and delaying the sole client inflow never improves earlier liquidity. Therefore the low-income/high-expense/latest-client path provides the worst balance for a fixed purchase decision. This reasoning is valid for this fixture; it is not a universal shortcut for taxes or contingent borrowing.

| Down payment | Payment date | Lowest cash under the complete stated set | Date of lowest cash | Reserve condition |
|---:|---|---:|---|---|
| 1,200,000 | November 15, 2026 | 1,080,000 | November 15, 2026 | Maintained under these assumptions |
| 1,300,000 | November 15, 2026 | 980,000 | November 15, 2026 | Breached by 20,000 |
| 1,400,000 | November 15, 2026 | 880,000 | November 15, 2026 | Breached by 120,000 |
| 1,200,000 | November 30, 2026 | 1,180,000 | January 15, 2027 | Maintained under these assumptions |
| 1,300,000 | November 30, 2026 | 1,080,000 | January 15, 2027 | Maintained under these assumptions |
| 1,400,000 | November 30, 2026 | 980,000 | January 15, 2027 | Breached by 20,000 |

A valid conclusion is:

> The 1,300,000 down payment on November 30 maintains the 1,000,000 floor through January 31 under the specified salary, expense, client-collection and settlement assumptions. It has 80,000 minimum headroom in this fixture. An earlier salary termination, an unpaid client invoice or omitted car ownership cost is outside this guarantee and requires another analysis.

The expected client payment does not increase today's cash. A stronger executable policy can also require that sufficient funds have actually settled before committing to the purchase.

**Relevant models:** M05, M07–M09, M13, M16.

### E04 — Equal failure frequency can hide very different severity

Consider loss equal to the maximum external funding gap:

| Loss | Approved scenario probability |
|---:|---:|
| 0 | 80% |
| 100,000 | 15% |
| 500,000 | 5% |

Expected loss is 40,000. At confidence level 90%, VaR is 100,000 under the lower-quantile convention. The worst 10% comprises the 5% probability at 500,000 and another 5% taken from the 100,000 atom:

$$
CVaR_{0.9}=\frac{0.05(500{,}000)+0.05(100{,}000)}{0.10}=300{,}000.
$$

Averaging only losses strictly greater than VaR would incorrectly give 500,000. These percentages are assumed probabilities for the fixture; assigning arbitrary stress weights would not establish a real 90% confidence level.

**Relevant models:** M10, M11; formulation foundation [R02](#r02), [R03](#r03).

### E05 — A multi-year tax comparison can beat the lowest-immediate-tax instinct

Use a **fictitious** annual marginal schedule: 10% on the first 100,000 and 30% above 100,000. Baseline taxable income is 60,000 in each of two years. Assume lawful discretionary taxable extraction of 100,000 in total, no other tax interactions, and spending dates that permit either timing pattern.

| Strategy | Taxable income, year 1 / year 2 | Tax, year 1 / year 2 | Incremental tax versus the two-year baseline |
|---|---|---|---:|
| Baseline | 60,000 / 60,000 | 6,000 / 6,000 | 0 |
| Extract 100,000 in year 1 | 160,000 / 60,000 | 28,000 / 6,000 | 22,000 |
| Extract 50,000 in each year | 110,000 / 110,000 | 13,000 / 13,000 | 14,000 |

The split reduces this modeled undiscounted incremental tax by 8,000. It is invalid as an answer when the household needs the full gross amount in year 1, the action is not legally deferrable, or the second year's rules differ. This is why both eligibility and dated liquidity must constrain tax minimization.

**Relevant models:** M23, M26, M29.

### E06 — A lower monthly payment is not the same as cheaper borrowing

For principal 1,000,000, a fixed effective monthly rate of 1%, twelve equal end-of-month payments, no fees and unrounded mathematical accrual:

$$
A=1{,}000{,}000\frac{0.01}{1-(1.01)^{-12}}
\approx88{,}848.78867834.
$$

A real contract may round each payment to 88,848.79 and adjust the final payment. The engine must replay those rounded payments, not assume the unrounded formula produces exactly zero principal under every convention.

For another warning, annual cash flows $(-100,230,-132)$ have **two** IRR roots: 10% and 20%. Therefore an IRR routine returning one root is not sufficient to rank arbitrary cash-flow patterns.

**Relevant models:** M30–M32. Contract/APR convention reference [R23](#r23).

### E07 — Company cash is not free household money

Company Alpha has 2,000,000 cash. Its modeled committed payroll is 700,000, tax remittance is 200,000 and a separate operating buffer is 600,000. A simple disjoint cash-constraint ceiling leaves 500,000 before any extraction costs.

However, **500,000 is not automatically legally distributable**. A validated dividend route might permit less, a documented shareholder-loan repayment might have different eligibility, and a prohibited route permits zero. Each route must be modeled independently.

If Alpha pays its household owner a 100,000 salary, Alpha's standalone cash decreases and the household's cash increases by the appropriate net amount. In a combined cash-flow boundary the internal portion cancels; payments to tax authorities remain external. The transfer does not manufacture group wealth.

**Relevant models:** M02, M04, M27, M41.

### E08 — A funding gap is not multiplied by its duration

Suppose a fixed one-pool path is 100,000 below its required floor on each of ten consecutive days and returns above the floor afterward.

$$
MinimumImmediateInjection=100{,}000,
\qquad IntegratedShortfall=1{,}000{,}000\ \text{currency-days}.
$$

The second measure expresses severity over time, not a requirement for 1,000,000 cash. With account-specific transfer restrictions, a separate multi-account optimization may produce a different injection requirement.

**Relevant models:** M03, M13.

<a id="verification"></a>
## 45. Verification and Acceptance Requirements

These are requirements for the future application, not a claim that the application already exists or passes them. Keep each test linked to its input fixture, expected result, rule version and model identifier. Numerical examples above provide a subset of reproducible regression fixtures.

### 45.1 Accounting, dates and boundary tests

| ID | Required acceptance case |
|---|---|
| V001 | A transfer between two included accounts conserves aggregate cash except explicit external costs. |
| V002 | A cross-currency transfer balances through explicit currency/clearing entries; unequal currency numbers are never directly canceled. |
| V003 | Adding an expected salary changes only future scenarios, not today's available money. |
| V004 | Reserving cash does not change ledger cash; paying and releasing the matching reserve does not double-reduce free cash. |
| V005 | Nested bank and emergency floors combine according to their coverage definition; disjoint obligations remain additive. |
| V006 | A company-owner transfer changes standalone ledgers and eliminates once, not twice, in the appropriate combined boundary. |
| V007 | A company's equity valuation and consolidated underlying assets are never both added to the same wealth measure. |
| V008 | Third-party shareholder interests and legal distribution constraints survive household aggregation. |
| V009 | A card purchase is expense recognition; its subsequent settlement is not another purchase expense. |
| V010 | Salary received after an intraday rent debit cannot fund the earlier debit. |
| V011 | Recurrence tests cover leap day, invalid monthly dates, last business day, timezones, cutoffs and holiday-version changes. |
| V012 | Partial receipt matching consumes only the fulfilled portion of a planned event and prevents duplicates. |

### 45.2 Taxes, contracts and funding tests

| ID | Required acceptance case |
|---|---|
| V013 | Bracket tests cover one minor unit below, at and above every threshold, plus credits, caps and rounding. |
| V014 | Gross-up output supplies at least the requested net amount after recomputed fees/withholding. |
| V015 | A non-monotone net-proceeds function does not use an unjustified monotone binary search. |
| V016 | Per-day or rolling tax thresholds aggregate all relevant transactions; artificial splitting cannot bypass the rule. |
| V017 | Creditable withholding reduces the compatible final balance due; final/noncreditable withholding is treated differently. |
| V018 | A tax refund is an asset/expected inflow until received, not current spendable money. |
| V019 | Current tax payable later is not mislabeled as accounting deferred tax. |
| V020 | An optimizer cannot erase unpaid tax or balloon debt by placing it one day after the horizon. |
| V021 | An ineligible dividend, fabricated reimbursement or undocumented loan route is excluded even when its nominal tax is lower. |
| V022 | Employer payroll, tax remittance and working-capital floors remain funded after owner extraction. |
| V023 | Tax-lot sales conserve quantity and adjust basis correctly; restricted losses do not create unsupported credits. |
| V024 | Foreign-tax credits and cross-entity loss relief are applied only with supported eligibility and limits. |
| V025 | The regular annuity fixture agrees with the general loan engine before rounding and with contract replay afterward. |
| V026 | Zero rates, negative rates where contracts permit them, floating resets, grace, balloon and prepayment cases are explicit. |
| V027 | IRR diagnostics identify the two-root fixture and report no-root cases instead of inventing a rate. |
| V028 | Credit-card tests cover grace loss, promotional expiry, multiple buckets and contractual payment order. |
| V029 | TCO does not count full price and down payment as two separate initial cash costs. |
| V030 | Rent/buy/refinance comparisons include equivalent horizons and terminal asset/debt positions. |

### 45.3 Uncertainty and decision tests

| ID | Required acceptance case |
|---|---|
| V031 | Every future conclusion lists its assumptions, horizon, uncertainty coverage and excluded shocks. |
| V032 | Three named scenarios never automatically receive a robust-envelope or probability label. |
| V033 | A proved monotone fixture reproduces E03; a non-monotone tax fixture rejects the same endpoint shortcut. |
| V034 | Enlarging an uncertainty set cannot expand robust feasibility when all other model components are fixed. |
| V035 | Increasing an independent hard reserve cannot improve the feasible optimum of a minimization problem with otherwise fixed inputs. |
| V036 | Shared-customer/default factors affect every linked company and household receipt consistently. |
| V037 | Whole-horizon probability is calculated jointly rather than copied from a single-date probability. |
| V038 | CVaR with probability mass at VaR reproduces E04 and uses the correct partial threshold mass. |
| V039 | Probabilities must be nonnegative and sum to one; absent probabilities remain absent. |
| V040 | DRO confidence statements are disabled unless the required sampling/model/radius conditions are documented. |
| V041 | Nonanticipativity forbids different current actions in scenarios with identical current observations. |
| V042 | Re-planning respects past actions and locked commitments and cannot rewrite history. |
| V043 | One-at-a-time thresholds are labeled as such; simultaneous adverse changes trigger a separate joint analysis. |
| V044 | First-breach detection includes intermediate dates; no breach through H is not reported as infinite runway. |
| V045 | A repeated 100,000 deficit reproduces E08 rather than multiplying the capital requirement by days. |
| V046 | Adding obligations beyond the horizon or changing terminal values produces an explicit sensitivity warning. |
| V047 | Censored invoice analysis distinguishes partial collections, defaults, write-offs and still-open invoices. |
| V048 | Portfolio-return sequences with identical compounded returns can produce different outcomes when withdrawals occur. |

### 45.4 Numerical integrity, explainability and governance tests

| ID | Required acceptance case |
|---|---|
| V049 | Tiny optimization instances are checked against an exhaustive independent oracle. |
| V050 | Claimed robust feasibility is checked with a valid full-set method, not an unresolved adversarial search. |
| V051 | Feasible incumbent, global bound, gap, residuals, search domain and approximation labels agree with actual solver output. |
| V052 | Exact decimal replay rejects plans that fail legal, cash or reserve checks after rounding. |
| V053 | Same preserved inputs/configuration reproduce the result within the documented platform contract; changes are identified. |
| V054 | Rule cycles, conflicting effective periods, invalid units and undefined denominators produce explicit errors. |
| V055 | Only user-soft constraints can be relaxed; accounting and verified legal requirements cannot be traded away. |
| V056 | Every displayed derived value can be traced to input values and intermediate operations without AI. |
| V057 | Historical forecast evaluation uses the original pre-outcome forecast and preserves the calibration/evaluation split. |
| V058 | Unknown jurisdiction, missing rule coverage or stale required inputs prevents unsupported legal/optimality claims. |
| V059 | Dominated alternatives are not presented as Pareto-efficient; scalar weights and tolerances remain disclosed. |
| V060 | A recommendation never sends money, changes legal ownership or commits a company transaction without a separately authorized execution capability. |


### 45.5 Privacy, authorization and disclosure tests

| ID | Required acceptance case |
|---|---|
| V061 | Changing visibility never changes legal/economic ownership, ledger postings or joint-account anti-double-counting. |
| V062 | A fully private account does not appear in unauthorized discovery, search, exports, notifications, scenario lists or calculation inputs. |
| V063 | A private-but-authorized account can contribute to a named household calculation without revealing its institution, account identifier, balance or transactions. |
| V064 | The owner can inspect complete provenance while another viewer receives only the authorized aggregate explanation for the same calculation context. |
| V065 | Balance-shared/transactions-private policy reveals the permitted balance but no transaction, merchant, category or transaction-derived explanation that reconstructs the history. |
| V066 | A 50/50 joint account remains economically 50/50 even when one owner has broader visibility than the other. |
| V067 | An account authorized only for `Buy Home` is unavailable to `Buy Car`, baseline forecasts and unrelated funding searches. |
| V068 | A private `Leave Job` scenario and its assumptions do not leak through scenario lists, household notifications, goal shifts or difference reports. |
| V069 | Company payroll viewers, employee recipients and household viewers receive different authorized representations of one linked movement without creating duplicate financial events. |
| V070 | Household membership does not grant company-account/payroll access; company role does not grant unrelated household access. |
| V071 | Effective-dated policy changes apply at the correct time and historical replay records the original policy version. |
| V072 | Revocation removes the object from new unauthorized calculations but does not rewrite historical ledger truth or previously committed transactions. |
| V073 | Difference/decomposition queries that would trivially reconstruct a restricted contribution trigger configured suppression/coarsening or require explicit authorization. |
| V074 | CSV export, API, report generation, cached result and notification paths enforce the same authorization policy as the main calculation view. |
| V075 | A funding optimizer cannot select a private account unless its calculation-access policy permits that purpose, even if the solver could otherwise improve the objective. |
| V076 | A tax optimizer cannot expose restricted income/account details through its explanation; it reports only viewer-authorized tax/funding contributions. |
| V077 | Missing, invalid or conflicting authorization policy fails closed and explains the authorization failure without exposing the protected object. |
| V078 | Privacy audit events record policy changes with actor, effective time, old/new policy identifiers and affected calculation contexts. |
| V079 | Broader authorization refines the explanation without changing the authoritative result when the permitted calculation input set is identical. |
| V080 | If two viewers have different calculation-access scopes, differing results are labeled with their calculation boundary so they are not falsely presented as contradictory calculations. |

### 45.6 Independent verification obligations

The calculation engine needs independent forms of checking: unit/contract tests; accounting invariants; dimensional analysis; property-based tests; small-instance exhaustive comparisons; scenario replay; adversarial input/rule tests; rule-pack official examples; and version-regression tests.

A sophisticated optimizer cannot compensate for incorrect bookkeeping. A correct ledger cannot compensate for an invalid tax rule. A verified tax rule cannot make a future invoice certain. These verification dimensions must be tested and reported separately.

Where a source paper has accessible code, reproduce a suitable small benchmark before adapting its method. Where only the abstract was inspected, obtain and review the complete formulation and conditions before implementing a claimed theorem-based guarantee. Research-inspired features can remain in the inventory without pretending that this implementation validation has already happened.

<a id="open-decisions"></a>
## 46. Research, Policy and Data Decisions Requiring Configuration

These are configuration and validation requirements, not questions that block the current discovery document.

| Decision | Why it matters | Required resolution |
|---|---|---|
| Jurisdictions, tax residency and company forms | Taxes and lawful extraction cannot be inferred from currency or timezone | Explicit user/entity configuration and verified official rules |
| Meaning of household wealth and business inclusion | Equity exposure differs from legal control and accessible cash | Named aggregation/valuation policies |
| Required planning horizons | Two-month liquidity and lifetime tax planning have different state/terminal needs | User-selected horizons plus material terminal obligations |
| Time precision | Monthly models can miss same-day failures | Contract-appropriate event/settlement granularity |
| Uncertainty philosophy | Stress sets and probabilities answer different questions | No probabilities by default; optional approved models |
| Joint shocks and information dates | Unrelated ranges can produce impossible or clairvoyant paths | Explicit dependencies and scenario-tree observations |
| Safety floor semantics | Overlap can double-count reserves or underfund separate needs | Earmark coverage and hard/soft constraint definitions |
| Objective priorities | Least tax may conflict with payroll or earliest purchase | User-defined objective hierarchy and risk constraints |
| Supported rule language | Arbitrary rules can invalidate monotonicity and convexity | Typed safe expressions with declared properties and tests |
| Solver methods and licensing | Some models need integer/global solvers; reproducibility varies | Method-specific evidence and no vendor assumption |
| Data sufficiency | Short histories do not establish robust probabilities | Sample/freshness warnings and user-approved assumptions |
| Legal/contract coverage | A source link is not a production-ready tax or payroll implementation | Rule-specific review and acceptance examples |
| Automation boundary | Planning is not execution authorization | Explicit separation and independently specified controls for execution |
| Privacy configuration UX | Final interaction design remains deferred, but authorization semantics do not | Implement object/context authorization, policy versioning and permission-aware provenance independently of the eventual UI |
| Privacy threat model | Application authorization is not automatically cryptographic privacy | Decide whether mutually distrustful users, compromised infrastructure or cross-party computation require stronger techniques |

### Final product statement after mathematical research

> Build an auditable, non-AI financial state and decision system for households and their companies. It must calculate present money exactly from verified records, model future cash and obligations as conditional paths, compare lawful actions using explicit objectives and constraints, and expose both the mathematics and the limits of every result **at the level each viewer is authorized to receive**. Restricted resources may participate in explicitly authorized shared calculations without forcing disclosure of unrelated private financial data. Advanced optimization and privacy controls must strengthen trust—not conceal assumptions, source data or authorization boundaries behind a more impressive formula.


<a id="references"></a>
## 47. Linked References and Evidence Notes

**Access date for this research pass:** September 10, 2026. Publication years refer to the cited edition/journal issue; online-first dates can differ. An evidence note records what was inspected, not a claim that every page was read. Sources with limited access remain useful discovery references but require full-formulation review before an implementation claims their guarantees.

### 47.1 Original research, author publications and foundational reports

<a id="r01"></a>
**R01 — Bertsimas, D.; Sim, M. (2004). _The Price of Robustness_. Operations Research, 52(1), 35–53.**  
[Publisher / DOI: 10.1287/opre.1030.0065](https://pubsonline.informs.org/doi/10.1287/opre.1030.0065) · [Author-hosted manuscript](https://web.mit.edu/dbertsim/www/papers/Robust%20Optimization/The%20price%20of%20Robustness.pdf)  
**Use:** Budgeted uncertainty, tractable robust formulations and explicit conservatism. **Evidence inspected:** Relevant manuscript formulation sections, including the uncertainty-budget equations, and publisher metadata. **Do not infer:** A probability of household success from an arbitrary uncertainty budget.

<a id="r02"></a>
**R02 — Rockafellar, R. T.; Uryasev, S. (2000). _Optimization of Conditional Value-at-Risk_. Journal of Risk, 2(3), 21–41.**  
[Publisher article](https://www.risk.net/journal-risk/2161159/optimization-conditional-value-risk) · [Author publication index](https://uryasev.github.io/publications/)  
**Use:** Variational CVaR objective and tail-loss optimization. **Evidence inspected:** Publisher record/summary and author bibliography; full manuscript was not successfully retrieved from the author download during this pass. **Implementation requirement:** Review the complete assumptions alongside R03 before using a claimed risk guarantee.

<a id="r03"></a>
**R03 — Rockafellar, R. T.; Uryasev, S. (2002). _Conditional Value-at-Risk for General Loss Distributions_. Journal of Banking & Finance, 26(7), 1443–1471.**  
[Publisher / DOI: 10.1016/S0378-4266(02)00271-6](https://www.sciencedirect.com/science/article/pii/S0378426602002716)  
**Use:** General/discrete loss distributions and the need to handle probability mass at the quantile. **Evidence inspected:** Publisher abstract/record, not the complete proof. The numerical discrete fixture in Section 44 was independently calculated.

<a id="r04"></a>
**R04 — Mohajerin Esfahani, P.; Kuhn, D. (2018; online 2017). _Data-driven distributionally robust optimization using the Wasserstein metric: performance guarantees and tractable reformulations_. Mathematical Programming, 171, 115–166.**  
[Open-access paper / DOI: 10.1007/s10107-017-1172-1](https://link.springer.com/article/10.1007/s10107-017-1172-1)  
**Use:** Distributional ambiguity, Wasserstein sets, supported tractable reformulations and conditional finite-sample certificates. **Evidence inspected:** Open full-text introduction, setup and guarantee/formulation discussions. **Do not infer:** That a small nonstationary household dataset meets the sampling and tail assumptions.

<a id="r05"></a>
**R05 — Boyd, S.; Busseti, E.; Diamond, S.; Kahn, R. N.; Koh, K.; Nystrup, P.; Speth, J. (2017). _Multi-Period Trading via Convex Optimization_. Foundations and Trends in Optimization, 3(1), 1–76.**  
[Author publication page](https://web.stanford.edu/~boyd/papers/cvx_portfolio.html) · [Author preprint, arXiv:1705.00109](https://arxiv.org/abs/1705.00109)  
**Use:** Multi-period planning, transaction costs and receding-horizon execution. **Evidence inspected:** Author/arXiv abstract and publication record. **Do not infer:** That the method predicts future market returns or that all household constraints are convex.

<a id="r06"></a>
**R06 — Johansson, K.; Boyd, S. (2025). _A Tax-Efficient Model Predictive Control Policy for Retirement Funding_. Journal of Retirement, 13(2), 56–92.**  
[Author publication page](https://stanford.edu/~boyd/papers/retirement.html) · [Author preprint, arXiv:2507.10603](https://arxiv.org/abs/2507.10603)  
**Use:** Tax-aware withdrawals, account transfers, terminal resources and repeated re-planning. **Evidence inspected:** Detailed author abstract and publication metadata; the author-page manuscript download failed during this pass. **Do not infer:** That simplified taxes cover arbitrary jurisdictions or that reported simulation performance guarantees actual retirement outcomes.

<a id="r07"></a>
**R07 — Moehle, N.; Kochenderfer, M. J.; Boyd, S.; Ang, A. (2021). _Tax-Aware Portfolio Construction via Convex Optimization_. Journal of Optimization Theory and Applications, 189(2), 364–383.**  
[Author publication page](https://stanford.edu/~boyd/papers/tax_aware_portfolio.html) · [Author-hosted full manuscript](https://stanford.edu/~boyd/papers/pdf/tax_aware_portfolio.pdf)  
**Use:** Tax-lot-aware trading and the distinction between exact formulations and convex/heuristic approximations. **Evidence inspected:** Relevant full-manuscript model/approximation passages and author metadata. **Do not infer:** Universal global optimality, or that costs omitted in a simplified paper may be omitted from executable household funding.

<a id="r08"></a>
**R08 — Woodruff, J.; Haskell, W. B.; Toriello, A. (2016). _Optimized Financial Systems Helps Customers Meet Their Personal Finance Goals with Optimization_. Interfaces, 46(4), 345–359.**  
[Publisher / DOI: 10.1287/inte.2016.0849](https://pubsonline.informs.org/doi/10.1287/inte.2016.0849)  
**Use:** Direct precedent for individual/couple personal-finance planning using multistage stochastic linear and mixed-integer optimization. **Evidence inspected:** Publisher abstract and bibliographic record. The application-specific event/tax rules here are proposed extensions.

<a id="r09"></a>
**R09 — Righetto, G. M.; Morabito, R.; Alem, D. (2016). _A robust optimization approach for cash flow management in stationery companies_. Computers & Industrial Engineering, 99, 137–152.**  
[Publisher / DOI: 10.1016/j.cie.2016.07.010](https://www.sciencedirect.com/science/article/pii/S0360835216302352) · [Author's university publication record](https://www.research.ed.ac.uk/en/publications/a-robust-optimization-approach-for-cash-flow-management-in-statio/)  
**Use:** Multi-account cash-flow network planning, uncertainty, costs and mixed-integer decisions in operating companies. **Evidence inspected:** Publisher abstract/introduction and university record; not the entire article. Transfer paths for this household product are an adaptation.

<a id="r10"></a>
**R10 — Salas-Molina, F.; Rodríguez-Aguilar, J. A.; Pla-Santamaria, D.; García-Bernabeu, A. (2021; online 2019). _On the formal foundations of cash management systems_. Operational Research, 21, 1081–1095.**  
[Publisher / DOI: 10.1007/s12351-019-00464-6](https://link.springer.com/article/10.1007/s12351-019-00464-6) · [University repository](https://riunet.upv.es/handle/10251/166959)  
**Use:** Formal multi-account cash-management structure. **Evidence inspected:** Publisher/repository abstract and metadata, not the complete formulation. Used as a requirements foundation, not a verified implementation blueprint.

<a id="r11"></a>
**R11 — Salas-Molina, F.; Rodríguez-Aguilar, J. A.; Pla-Santamaria, D. (2020). _A stochastic goal programming model to derive stable cash management policies_. Journal of Global Optimization, 76, 333–346.**  
[Publisher / DOI: 10.1007/s10898-019-00770-5](https://link.springer.com/article/10.1007/s10898-019-00770-5)  
**Use:** Multiple objectives, cash-policy stability and multi-account transitions. **Evidence inspected:** Publisher abstract and bibliographic information. User-specific goal priorities and fairness policies are not supplied by the paper.

<a id="r12"></a>
**R12 — Shang, D.; Kuzmenko, V.; Uryasev, S. (2018; online 2016). _Cash flow matching with risks controlled by buffered probability of exceedance and conditional value-at-risk_. Annals of Operations Research, 260, 501–514.**  
[Publisher / DOI: 10.1007/s10479-016-2354-6](https://link.springer.com/article/10.1007/s10479-016-2354-6) · [Author's university record](https://researchconnect.suny.edu/en/publications/cash-flow-matching-with-risks-controlled-by-buffered-probability-/)  
**Use:** Matching liabilities with instrument cash flows while controlling shortfall risks. **Evidence inspected:** Publisher and university abstracts/metadata. Applying the method to household deposits or payroll ladders is a proposed adaptation.

<a id="r13"></a>
**R13 — Baumol, W. J. (1952). _The Transactions Demand for Cash: An Inventory Theoretic Approach_. Quarterly Journal of Economics, 66(4), 545–556.**  
[Publisher / DOI: 10.2307/1882104](https://academic.oup.com/qje/article/66/4/545/1938624)  
**Use:** Classical fixed-transfer-cost versus idle-cash-cost benchmark. **Evidence inspected:** Publisher record/preview, not paywalled full text. The one-variable cost minimization is explicitly stated in M44 and can be independently differentiated; it is not the general application model.

<a id="r14"></a>
**R14 — Miller, M. H.; Orr, D. (1966). _A Model of the Demand for Money by Firms_. Quarterly Journal of Economics, 80(3), 413–435.**  
[Publisher / DOI: 10.2307/1880728](https://academic.oup.com/qje/article-abstract/80/3/413/1876608)  
**Use:** Classical stochastic cash-control bands as a benchmark candidate. **Evidence inspected:** Publisher record/section preview, not the complete derivation. Review the original stochastic assumptions before implementing this benchmark; it must not supersede known dated obligations.

<a id="r15"></a>
**R15 — Dixit, A. K.; Pindyck, R. S. (1994). _Investment under Uncertainty_. Princeton University Press.**  
[Publisher-book record and chapter access via JSTOR](https://www.jstor.org/stable/j.ctt7sncv)  
**Use:** Irreversibility, waiting and information in investment decisions. **Evidence inspected:** Book description/introductory preview, not the complete monograph. Household option values require their own feasible actions, information and valuation assumptions.

<a id="r16"></a>
**R16 — Shapley, L. S. (1952). _A Value for N-Person Games_. RAND report P-295.**  
[Original institutional report record](https://www.rand.org/pubs/papers/P295.html)  
**Use:** Cooperative value allocation. **Evidence inspected:** RAND description and bibliographic record. This is the 1952 report; the well-known book-chapter publication followed in 1953. The proposed expense/savings allocation does not claim that an axiomatic value establishes moral fairness.

<a id="r32"></a>
**R32 — Ruszczyński, A. (2010). _Risk-averse dynamic programming for Markov decision processes_. Mathematical Programming, 125, 235–261.**  
[Publisher / DOI: 10.1007/s10107-010-0393-3](https://link.springer.com/article/10.1007/s10107-010-0393-3)  
**Use:** Conditional risk measures and risk-averse dynamic-programming equations. **Evidence inspected:** Publisher abstract and bibliographic record. Implementation must verify the state, risk and convergence assumptions rather than treating any static CVaR objective as time-consistent.

### 47.2 Official financial, actuarial and technical sources

<a id="r17"></a>
**R17 — Kneifel, J. D.; Webb, D. (2022). _Life Cycle Costing Manual for the Federal Energy Management Program_. NIST Handbook 135e2022-upd1.**  
[Official NIST publication](https://www.nist.gov/publications/life-cycle-costing-manual-federal-energy-management-program-0) · [DOI: 10.6028/NIST.HB.135e2022-upd1](https://doi.org/10.6028/NIST.HB.135e2022-upd1)  
**Use:** Consistent lifecycle cost, discounting and alternative comparison. **Evidence inspected:** Official publication description/record. The source concerns federal energy investments; household/vehicle uses here are methodological adaptations, not its prescribed domain.

<a id="r18"></a>
**R18 — NIST/SEMATECH e-Handbook. _Kaplan–Meier estimation_.**  
[Official method explanation](https://itl.nist.gov/div898/handbook/apr/section2/apr215.htm)  
**Use:** Survival estimation with censored observations. **Evidence inspected:** Method page and formula. Invoice timing is a proposed adaptation and requires appropriate censoring/event assumptions.

<a id="r19"></a>
**R19 — IFRS Foundation. _IAS 7: Statement of Cash Flows_.**  
[Official standard overview](https://www.ifrs.org/issued-standards/list-of-standards/ias-7-statement-of-cash-flows/)  
**Use:** Cash-flow distinctions and treatment of cash versus non-cash changes. **Evidence inspected:** Official overview, not a statutory reporting implementation review. The application is not declared IFRS-compliant.

<a id="r20"></a>
**R20 — IFRS Foundation. _IAS 12: Income Taxes_.**  
[Official standard overview](https://www.ifrs.org/issued-standards/list-of-standards/ias-12-income-taxes/)  
**Use:** Current versus deferred tax terminology and tax-asset/liability distinctions. **Evidence inspected:** Official overview. This is not a source of local tax brackets or extraction eligibility.

<a id="r21"></a>
**R21 — IFRS Foundation. _IFRS 10: Consolidated Financial Statements_.**  
[Official standard overview](https://www.ifrs.org/issued-standards/list-of-standards/ifrs-10-consolidated-financial-statements/)  
**Use:** Control-based financial-reporting consolidation as distinct from a household analytical grouping. **Evidence inspected:** Official overview. No inference of tax consolidation rights is permitted.

<a id="r22"></a>
**R22 — IFRS Foundation. _IFRS 9: Financial Instruments_.**  
[Official standard overview](https://www.ifrs.org/issued-standards/list-of-standards/ifrs-9-financial-instruments/)  
**Use:** Financial-instrument and impairment concepts; keeping provisions distinct from cash movements. **Evidence inspected:** Official overview and related official educational material. No compliant expected-credit-loss implementation is claimed.

<a id="r23"></a>
**R23 — US Consumer Financial Protection Bureau. _Regulation Z, Appendix J: Annual Percentage Rate Computations for Closed-End Credit Transactions_.**  
[Official regulation and mathematical conventions](https://www.consumerfinance.gov/rules-policy/regulations/1026/J)  
**Use:** A concrete official example of cash-flow/timing conventions for APR. **Evidence inspected:** Official technical regulation page. US-specific; neither universal law nor a substitute for local contract/rate definitions.

<a id="r24"></a>
**R24 — US Consumer Financial Protection Bureau. _Credit card key terms_.**  
[Official credit-card concepts](https://www.consumerfinance.gov/consumer-tools/credit-cards/answers/key-terms/)  
**Use:** Grace periods, interest and related credit-card distinctions. **Evidence inspected:** Official explanation. Actual card contracts and applicable local law control.

<a id="r25"></a>
**R25 — Basel Committee on Banking Supervision (2018). _Stress testing principles_. Bank for International Settlements.**  
[Official principles](https://www.bis.org/publications/201810-guidelines-stress-testing-principles)  
**Use:** Scenario governance, limitations and stress-program discipline. **Evidence inspected:** Official publication overview. Principles inspire requirements; bank regulatory obligations are not imposed on households.

<a id="r26"></a>
**R26 — Basel Committee on Banking Supervision (2008). _Principles for Sound Liquidity Risk Management and Supervision_. Bank for International Settlements.**  
[Official publication](https://www.bis.org/publ/bcbs144.htm)  
**Use:** Liquidity cushions, intraday needs and contingent sources. **Evidence inspected:** Official publication overview. The household/company reserve rules remain user- and contract-specific.

<a id="r27"></a>
**R27 — Desruisseaux, B. (2009). _Internet Calendaring and Scheduling Core Object Specification (iCalendar)_. RFC 5545.**  
[Official IETF specification](https://datatracker.ietf.org/doc/html/rfc5545)  
**Use:** Recurrence representation and exception semantics. **Evidence inspected:** Relevant recurrence passages. Financial business-day adjustment is an additional contract-specific requirement.

<a id="r28"></a>
**R28 — Gurobi Optimization. _Tolerances and User-Scaling_.**  
[Official numerical guidance](https://docs.gurobi.com/projects/optimizer/en/current/concepts/modeling/tolerances.html)  
**Use:** Finite precision, scaling, feasibility and interpretation of mathematical-programming results. **Evidence inspected:** Official guidance. Cited as a concrete solver reference, not a mandated vendor or proof that all solver classes provide exact certificates.

<a id="r29"></a>
**R29 — Gurobi Optimization. _Infeasibility Analysis_.**  
[Official documentation](https://docs.gurobi.com/projects/optimizer/en/current/features/infeasibility.html)  
**Use:** Infeasible subsystems and controlled feasibility relaxation. **Evidence inspected:** Official documentation. An irreducible conflict is not necessarily unique or minimum-cardinality.

<a id="r30"></a>
**R30 — Gurobi Optimization. _Is Gurobi deterministic?_**  
[Official reproducibility guidance](https://support.gurobi.com/hc/en-us/articles/360031636051-Is-Gurobi-deterministic)  
**Use:** Model order, configuration, machine/version, time limits and deterministic execution caveats. **Evidence inspected:** Official support explanation. The application needs its own documented reproducibility contract.

<a id="r31"></a>
**R31 — US Social Security Administration. _Actuarial Life Table_.**  
[Official actuarial table and explanation](https://www.ssa.gov/oact/STATS/table4c6.html)  
**Use:** Example of an authoritative mortality-table input. **Evidence inspected:** Official table/explanation; no particular mortality values are prescribed in this document. Population, period/cohort basis and suitability must be declared.

<a id="r33"></a>
**R33 — Gurobi Optimization. _Multiple Objectives_.**  
[Official multi-objective documentation](https://docs.gurobi.com/projects/optimizer/en/current/features/multiobjective.html)  
**Use:** Blended versus hierarchical objectives, weights and permitted objective degradation. **Evidence inspected:** Official technical documentation. User priorities and Pareto-search design remain application requirements, not vendor defaults.

### 47.3 Carried-forward product research

These links preserve the prior competitive-requirements baseline. They are reference patterns, not recommendations to abandon the proposed application, and this pass did not independently re-audit every current product capability. Product behavior, supported jurisdictions and platform availability may change.

<a id="p01"></a>
**P01 — PocketSmith.** [What-if scenarios and planning](https://www.pocketsmith.com/plan-ahead/what-if-scenarios/). Used for dated forecasting and scenario concepts.

<a id="p02"></a>
**P02 — ProjectionLab.** [Tax analytics](https://projectionlab.com/tax-analytics). Used for inspectable tax planning.

<a id="p03"></a>
**P03 — ProjectionLab.** [Roth-conversion/optimization help](https://projectionlab.com/help/roth-conversion). Used for explicit strategy objectives; jurisdiction-specific retirement structures are not assumed universally applicable.

<a id="p04"></a>
**P04 — RightCapital.** [Withdrawal strategy](https://help.rightcapital.com/knowledge-base/client-portal/tax/tax-strategies/withdrawal-strategy). Used for comparing account withdrawal sequences.

<a id="p05"></a>
**P05 — RightCapital.** [Tax strategies](https://help.rightcapital.com/module-overview/client-portal/tax/tax-strategies). Used for strategy and tax-detail requirements.

<a id="p06"></a>
**P06 — Holistiplan.** [Modeling Roth conversions](https://help.holistiplan.com/modeling-roth-conversions). Used for baseline/incremental-tax comparison concepts.

<a id="p07"></a>
**P07 — Actual Budget.** [Schedules](https://actualbudget.org/docs/tour/schedules/). Used for recurrence flexibility.

<a id="p08"></a>
**P08 — Monarch Money.** [Monarch for couples](https://help.monarch.com/hc/en-us/articles/20926382202004-Monarch-for-Couples). Used for household ownership/shared-context requirements. This product extends the pattern by separating economic ownership, visibility and calculation access.

<a id="p09"></a>
**P09 — YNAB.** [Assigning future income: an overview](https://support.ynab.com/en_us/assigning-future-income-an-overview-BJsTo0jCq). Used for the distinction between currently held money and expected future income.

<a id="p10"></a>
**P10 — Quicken Simplifi.** [Projected cash flow](https://www.quicken.com/features/projected-cashflow/). Used for scheduled bills/income and account cash-flow concepts.

---

**End of requirements analysis, research-expanded version 3.2.** All feature candidates remain part of the discovery inventory. Deferred and intentionally unspecified items are listed in Section 22; foundational authorization and permission-aware provenance remain required.
