# Atlas Financer — verbose UI implementation plan

**Status:** delivery plan for the desktop UI, derived from `plan.md` (requirements v3.2).
**Stack:** Rust + gpui via gpui-kit 0.6 (components + theme tokens only), desktop on macOS / Linux / Windows.
**Repo:** `atlas/` — Cargo workspace `crates/atlas-core` (engine, no UI) + `crates/atlas-app` (the window).
**Board:** DevBench tasks, one parent task per milestone (M0–M11), one subtask per work item below.

---

## 1. What "verbose UI" means here

`plan.md` defers UX design (§22.1) but makes explainability non-negotiable (§2.1, §2.6, §24):
*every derived value must expose its complete inputs and calculation*, and *future money is
never silently treated as available money* (§2.4). A verbose UI is the UI that takes those two
rules literally in this phase:

1. **Every number is a `Calc`.** Nothing on screen is a bare figure. Each derived value carries
   its calculation graph and renders a "Why?" affordance that opens the chain exactly in the
   layout §2.1 uses (`Current reconciled liquid cash 1,400,000 + contractually expected future
   salary 600,000 − … = …`).
2. **Every value is labelled.** Money class (§2.4: confirmed / reserved / free / expected /
   conditional), certainty (§10.3) and result strength (§32.2: exact accounting, conditional
   path, scenario-tested, robust-feasible, best-on-grid, …) are shown as gpui-kit `Tag`s next
   to the value, not hidden in tooltips.
3. **Every assumption is listed.** Any forward-looking screen has an assumptions panel (§10.2)
   and uses the conditional phrasing of §10.1 ("could … while retaining … assuming …").
4. **Every domain object has a screen.** People, companies, accounts, reservations, event
   series, occurrences, assumptions, rules, tax rules, scenarios, decisions, access policies
   and audit events (§27) are all browsable and inspectable; nothing lives only inside the engine.
5. **Code is explicit.** Long names, one concept per module, comments that cite the plan
   section they implement (e.g. `// §6.6 free current cash`). UX polish is not a goal yet
   (`AGENTS.md`); correctness, inspectability and screenshot-ability are.

Numbers in the fixture household follow the plan's own examples (E01–E08, §2.1, §13.5, §19)
so every screen can be checked against the document by eye.

---

## 2. Architecture

```
atlas/
├── Cargo.toml                 workspace (resolver 2, dev profile tuned for gpui)
├── crates/atlas-core/         engine: money, ids, vocabularies, household model,
│                              reservations, timeline, forecast, provenance, authz, fixtures
├── crates/atlas-app/          gpui-kit window: shell, screens, explain sheet, alerting hook
├── docs/                      this plan + later design notes (mirrored to DevBench docs)
├── scripts/shoot.sh           headless screenshots of every screen (uses ../gpui-lab/gpui-shot)
└── shots/                     PNG output (gitignored; shared via `devbench media put`)
```

**atlas-core** has no gpui, no I/O and no floating-point money. `Money` is integer minor units
plus a currency code; cross-currency arithmetic is a typed error (§32.1). `Calc<T>` pairs a
value with a `ProvNode` graph; `ProvNode::render_chain()` produces the §2.1 text
deterministically (no LLM, §42). Authorization (`Viewer`, `AccessPolicy`, `Disclosure`) is
part of the model from M0 because the plan makes it foundational (§7.1, §22.2), even though
the policy *editor* comes in M10; `ProvNode::project(viewer)` is the M55 projection.

**atlas-app** owns only layout and interaction. Every control is a gpui-kit component and every
colour a `cx.theme()` token. Screens are one module each under `screens/`; shared pieces
(`explain` sheet, `labels` for the three tag vocabularies, `money_text`) live in `widgets/`.
Failures (panics, engine invariant violations) go through `alerting.rs` to the DevBench notify
endpoint when `DEVBENCH_NOTIFY_URL` / `DEVBENCH_NOTIFY_TOKEN` are set, and to the log otherwise.

### Screen map (sidebar)

| Sidebar section | Screen(s) | Plan sections |
|---|---|---|
| Household | overview: people, companies, accounts, liquidity strip, provenance | §5, §6, §7 |
| People / Companies / Accounts | entity lists + detail with balance definitions | §7, §8 |
| Liquidity & reservations | money definitions per boundary, earmarks, headroom | §6, §17, M02 |
| Timeline | series, occurrences, planned vs actual, transfers | §9, §15, §16, M05 |
| Projections | chronological forecast per boundary, lowest balance, chain | §11, §2.1 |
| Assumptions | register, ranges, derivations, sensitivity | §10 |
| Taxes | rule packs, tax events, incremental tax | §12, M23–M24 |
| Rules | user rules, conflicts, simulation | §14 |
| Scenarios | overlays, comparison | §18 |
| Decisions | funding optimizer, affordability grid, goals | §13, §19, §20, §26 |
| Privacy | viewer switcher, policies, audit log | §7.1–7.6, §18.5, M55 |
| Settings | theme, fixture selection, alert status | — |

---

## 3. Milestones and tasks

Each milestone ships with unit tests in `atlas-core`, at least one gpui-kit UI test in
`atlas-app`, logging around its engine calls, and headless screenshots posted to the chat.
Acceptance ids (E0x, V0xx) refer to `plan.md` §44–§45.

### M0 — Foundation: workspace, core model, provenance kernel, app shell
1. Cargo workspace in `atlas/` (`atlas-core`, `atlas-app`), Linux-box plumbing, README, `scripts/shoot.sh`.
2. `atlas-core::money` — integer minor-unit `Money`, currency-checked arithmetic, plan-style formatting; typed ids; vocabularies `MoneyClass`, `Certainty`, `ResultStrength`.
3. `atlas-core::provenance` — `Calc<T>` + `ProvNode` graph, deterministic chain rendering (§2.1 layout), disclosure levels and `project(viewer)` (M55 skeleton).
4. `atlas-core::model` — Household, Person, Company, Account, OwnershipShare, Reservation, AccessPolicy skeleton; `fixtures::plan_household()` mirroring §2.1/E01/E03/E07 numbers.
5. `atlas-app` shell — TitleBar, Sidebar with every section, StatusBar, theme toggle, Root layers; navigation state; Household overview screen; Explain sheet widget.
6. Monitoring — structured logging, panic hook + invariant reporter posting to `DEVBENCH_NOTIFY_URL/api/notify/` with a server-side token from env; unit-tested against a local HTTP stub.
7. Tests, screenshots, push, MR — core unit tests, UI test for shell/navigation/explain, light+dark screenshots via gpui-shot, `git push`, MR against `main`.

**Acceptance:** E01 arithmetic reproduced in core; household free cash excludes company cash (§8.5); every figure on the overview opens a chain; `cargo test` green; screenshots posted.

### M1 — Entities & accounts (verbose views)
1. People screen — roles, owned/co-owned accounts with shares, income sources, companies.
2. Companies screen — separate ledger: accounts, employees/payroll, constraints (payroll reserve, working-capital floor), "extractable cash" placeholder with the §8.5/M27 legal-capacity caveat.
3. Accounts screen — the full §7 property table incl. liquidity class, minimum balance, transfer delay, fees, tax treatment, source of truth, last reconciliation, calculation access.
4. Account detail — balance-definition strip (ledger / pending / reserved / free / available-for-purpose), reservations and series touching the account.
5. Tests — joint ownership counted once at household level (V061/V066), company cash excluded (E07), UI test for account detail.

### M2 — Liquidity & reservations (E01, E08)
1. Reservation model — coverage sets, nested vs disjoint floors, hard vs soft (§17, M02); release on payment (E01).
2. Liquidity screen — total assets, liabilities, net worth, liquid, reserved, free per boundary (account / person / company / household), each with its chain (§6).
3. Negative headroom preserved as a warning; zero-floored display reports the deficit separately (§6.6).
4. Reservation editor dialog — name, account, amount, coverage, hardness, purpose.
5. Tests — E01, nested floors (V005), reserve/release (V004), first breach + minimum injection (E08, V045).

### M3 — Timeline & scheduling engine
1. Recurrence engine — one-time, daily, weekly / N-weekly, monthly / N-monthly, specific days, quarterly, annual, until / count / indefinite; explicit invalid-date policy (§9.1, M05).
2. Effective-dated amount changes and exceptions — edit one / this-and-future / series, skip, move (§9.2–9.3).
3. Occurrence clocks — contractual due, posting, settlement, availability; explicit same-day ordering (§11.1).
4. Timeline screen — chronological table with entity / account / certainty / status filters; series editor dialog; transfers as one linked movement (§15).
5. Planned vs actual — reconciliation links and statuses (planned, due, partial, fulfilled, skipped, cancelled, overdue); fulfilled occurrences never counted twice (§16, V012).
6. Tests — recurrence edge cases (V011), employment transitions (§9.4), transfer conservation (V001), card purchase vs settlement (V009).

### M4 — Forecast & projections
1. Chronological projection engine per boundary through a horizon; postings once; reservations constrain but do not post (§11.1).
2. Lowest projected balance and date, accounts that go negative, reserve breaches, required transfer points (§11.3).
3. Projections screen — AreaChart per boundary, the §2.1 chain (starting cash + contractual + assumed − …), assumptions panel; expected money never shown as available (§2.4, V003).
4. Named cases conservative / expected / optimistic as scenario paths labelled "Scenario-tested", never robust (§10.6, V032).
5. Forecast provenance record — snapshot, event set, rules, tax versions, assumptions, policy versions, timestamp; reproducible re-run (§11.4, §24).
6. Tests — E03 fixture table, intraday ordering (V010), V003, V044.

### M5 — Assumptions, uncertainty & sensitivity
1. Assumption register — kinds (§10.3), source, acceptance, expiry / freshness (F117).
2. Amount and date ranges; historically derived assumptions with formula and sample disclosure (§10.4–10.7).
3. Result-strength labels (§32.2) on every derived value; conditional phrasing templates (§10.1).
4. Sensitivity screen — one-at-a-time breakpoints with the joint-analysis caveat; joint reverse-stress placeholder (§10.8, M14).
5. Tests — V031, V043, derived-assumption provenance.

### M6 — Taxes
1. Tax rule model — jurisdiction, type, effective range, brackets / rate / formula, exemptions, scope, source metadata, pack version (§12.1); bracket function M23.
2. Tax rule pack screen — versions and effective dates; DEMO packs labelled fictitious; user-defined rules with explicit threshold semantics (§12.2, §14.3).
3. Tax liabilities as timeline events — immediate, withholding, accrued-payable-later, instalments; tax-reserve link; entity attribution (§12.3–12.6).
4. Incremental tax panel — with vs without action (§12.5); E05 multi-year comparison.
5. Tests — brackets at thresholds (V013), creditable withholding (V017), E05, rule-version pinning (§25).

### M7 — Rules engine
1. Rule model — name, scope, trigger, conditions, action, priority, effective range, enabled, scenario applicability, explanation, version (§14.1–14.2).
2. Rules screen — list, editor, version history.
3. Conflict-resolution inspector — priority → specificity → tie-break, decision visible (§14.7).
4. Funding and bank-selection rules evaluated by the forecast / funding engines (§14.5–14.6).
5. Rule simulation — "if this rule had been active …" diff of balances and taxes (§14.8).
6. Tests — deterministic conflicts, cycle / conflicting effective periods error (V054), simulation diff.

### M8 — Scenarios & comparison
1. Scenario overlay model — add / remove / change events, end streams, tax-rule and funding-rule changes, assumptions (§18.2).
2. Scenario composition with compatibility check (§18.3).
3. Scenarios screen — baseline vs scenario side by side; comparison metrics table (§18.4).
4. Scenario difference attribution without double counting (F139).
5. Tests — overlay determinism, comparison metrics, composition conflicts.

### M9 — Funding optimizer & affordability decisions
1. Funding request model and candidate enumeration over permitted sources; gross-up to net (M25, E02).
2. Objectives and constraints editor (§13.2–13.3); search-space disclosure and solution status ("Best on specified grid", §13.5, §32.2).
3. Strategy comparison screen in the §13.5 format; company extraction routes with legal-capacity caveat (E07, M27).
4. Affordability decision screen — price, window, down-payment range, reserve → grid evaluation (E03) → conditional result with assumptions (§19).
5. Goals and trade-offs — goal delays caused by a decision (§20); recommendation contract rendering (§26).
6. Tests — E02, E03 grid, V014, V035, cheapest-tax path that violates the reserve is rejected.

### M10 — Privacy & authorization
1. AccessPolicy, AccessGrant, DisclosurePolicy, HouseholdRole, EntityRole — effective-dated and versioned (§5.13–5.18, §7.5).
2. Viewer switcher in the title bar ("view as …"); every screen filters through policy (§7.1–7.3).
3. Provenance projection — aggregate-only nodes, redaction, suppression rule for difference attacks (M55, §7.6).
4. Policy editor — presets, per-field visibility, calculation access, purpose-specific grants (§7.2–7.4).
5. Privacy audit log screen (§5.18); fail-closed messages (F162).
6. Tests — V061–V064, V067, V071, V073, V077.

### M11 — Verification & handover
1. Acceptance fixtures E01–E08 as regression tests tagged with model ids.
2. UI integration tests for every screen (gpui-kit `test-support`).
3. Screenshot script covering every screen in light and dark; reference PNGs in DevBench media.
4. Recorded walkthrough (gpui-shot step sequence) shared in the chat.
5. Docs — README, `AGENTS.md`, knowledge-base doc; final MR against `main`.

---

## 4. Testing and monitoring standard (every milestone)

- **Engine:** unit tests next to the code; fixtures reproduce the plan's worked examples with
  the exact numbers; property tests where the plan states invariants (conservation, no double
  counting, monotonicity where proved).
- **UI:** `#[gpui_kit::test]` integration tests (`test-support`) that open the view, click
  through navigation and assert on rendered ids/values; pixel checks through `gpui-shot` on
  Linux (`scripts/shoot.sh`).
- **Monitoring:** `log` at `info` around every engine entry point (which boundary, horizon,
  policy version, fixture) and at `error` on every failure path; `alerting::report()` on
  panics and violated invariants posts `{text, level, source: "atlas-app"}` to
  `$DEVBENCH_NOTIFY_URL/api/notify/` with `Authorization: Bearer $DEVBENCH_NOTIFY_TOKEN`.
  The token is read from the environment only; a missing configuration logs a warning once
  and degrades to local logging.

---

## 5. Building

macOS (Mujtaba): `cd atlas && cargo run --bin atlas` — plain toolchain, no extra setup.

Linux DevBench box (agents): `source ~/.cargo/env`; the vendored `.sysroot` symlink points at
`../gpui-lab/.sysroot` (created by `gpui-lab/scripts/setup-linux-sysroot.sh`), `.cargo/config.toml`
wires it into the linker, and `~/.cargo/config.toml` keeps `jobs = 2` for the 2 GiB cgroup.
Screenshots: `scripts/shoot.sh` (uses `../gpui-lab/target/debug/gpui-shot`).

---

## 6. Open decisions (asked in chat, not blocking)

1. Baseline theme for Atlas: gpui-kit default light/dark (current) or one of the bundled JSON themes.
2. Whether `gpui-lab/` moves into the repo (e.g. `atlas/tools/gpui-lab`) now that product code lives in `atlas/`.
3. `test-support` on the main gpui-kit dependency (single build on the small box) vs dev-dependency (double build) — kept on the main dependency until the box has headroom.
4. Fixture currency and presentation: the plan's examples are unit-less whole numbers; the fixture uses `PKR`-style whole units and shows minor units only when non-zero.
