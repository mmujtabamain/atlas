# Atlas Financer

A deterministic, user-controlled financial timeline and decision engine for households,
their people, accounts and companies — the product specified in [`plan.md`](plan.md)
(requirements v3.2). Desktop app in **Rust + gpui via gpui-kit 0.6**.

No AI anywhere in the calculation path. Every derived figure carries its complete
calculation chain and can answer *"why is this number this number?"* (§2.1); future
money is never shown as money already available (§2.4); authorization is part of the
data model, not a later feature (§7.1).

## Layout

| Path | What |
|---|---|
| `crates/atlas-core` | Engine: money (integer minor units), household model, reservations, timeline, forecast, provenance graph, authorization, taxes, rules. No UI, no I/O. |
| `crates/atlas-store` | Persistence: one SQLite file per household, backups, lock file. |
| `crates/atlas-app` | The gpui-kit window: shell, screens, the explain sheet, data entry, failure alerting. Binary `atlas`. |
| `docs/ui-implementation-plan.md` | Milestones M0–M11 and their tasks (mirrored on the DevBench board). |
| `scripts/shoot.sh` | Headless screenshots of every screen, light and dark, plus dialogs (Linux box). |
| `scripts/walkthrough.sh` | The captioned walkthrough video (`shots/walkthrough.mp4`) from a gpui-shot step sequence. |
| `docs/handover.md` | What was built per milestone, how to run and test, known limits. |
| `plan.md` | The requirements document this repo implements. |

## Build and run

```bash
cargo run --bin atlas -- --theme dark --screen household --viewer a
cargo test                      # engine unit tests, E01–E08 examples, store, app UI integration tests
```

Options: `--theme light|dark`, `--size WxH`, `--screen <section>` (see `atlas --help`),
`--viewer a|b` (which fixture person is looking — private objects project to aggregates),
`--no-perf-overlay` (hide gpui's frame-time readout, see below).

### Logs and the frame meter (performance work)

Every run logs to **stderr and `logs.log`** in the current directory (`cargo run` → the
repository root; the previous run is kept as `logs.prev.log`). Send `logs.log` when the app
feels slow: it starts with the build profile (debug/release, opt-level), OS, window size and
scale factor, then every engine computation with its duration (`perf: compute … took 12.2ms`)
and, per frame, what a frame cost:

```text
perf: frame #12 section=timeline build=4.7ms draw≈95.3ms interval=101.0ms content(clone=0.0ms render=2.6ms) input(moves=3 wheel=0)
perf: summary 1.0s: 9 frames (fps≈8.6) build avg=2.0ms max=9.4ms · draw≈ avg=64.7ms max=110.7ms · slow(>50ms)=5 … | gpui draw p50=71.4ms p90=113.1ms max=113.1ms n=9 · dirty→present p50=133.1ms …
```

- `build` — time in `AtlasApp::render` (our element tree). `draw≈` — build + gpui layout + paint,
  measured to a probe painted last in the tree. `gpui draw` — gpui's own `Window::draw` histogram
  (the `profiler` feature), summarised once a second while frames happen.
- The **status bar** shows the previous frame: `11 fps · build 1 ms · draw 49 ms · frame #50`.
  gpui only draws when something changed, so "fps" is the rate *while frames are happening*;
  an idle window at 0 is healthy. The green **overlay** in the top-right corner is gpui's own
  frame-time readout (current, 1 %/10 % worst, max, frame count); `--no-perf-overlay` or
  `ATLAS_PERF_OVERLAY=0` hides it.
- Filters: `RUST_LOG=<filter>` (both sinks; the file keeps `atlas_*=debug` on top of it),
  `ATLAS_LOG_FILE_FILTER=<filter>` (file only), `ATLAS_LOG_FILE=<path>|off`.
- Module docs: `crates/atlas-app/src/perf.rs` and `logging.rs`.

The app boots with the fictitious *plan household* fixture (`atlas_core::fixtures`), whose
numbers come from the plan's own worked examples (E01, E03, E07, §13.1, §10.2).

### Linux DevBench box

The box has no root and no GPU. `source ~/.cargo/env` first; the linker finds the vendored
system libraries through `.cargo/config.toml` and the `.sysroot` symlink into
`../gpui-lab/.sysroot` (built by `gpui-lab/scripts/setup-linux-sysroot.sh`). Keep
`~/.cargo/config.toml` at `jobs = 2` (2 GiB memory cgroup). Screenshots:
`scripts/shoot.sh [scenario]`, then `devbench media put shots/<name>.png`.

## Your data (M12)

The app starts with the fictitious sample household. **Household ▸ New household…** creates an
empty one (name, base currency — USD by default — reconciliation date, first person); every
screen then has a **New …** button (people, companies, accounts with the §7 properties, event
series with any recurrence, assumptions, scenarios, actual transactions with reconciliation,
tax rules) and account detail has **Reconcile…** and **Delete**.

- **Household ▸ Save as…** writes one SQLite file, `<name>.atlas.sqlite`, wherever you choose
  (default `~/Documents/Atlas/`). **Save** rewrites it in one transaction after copying the
  previous version into `backups/` (20 kept). Open it again with **Household ▸ Open…** or
  `atlas --household /path/to/ours.atlas.sqlite`.
- **One editor at a time.** A sidecar `.lock` file names who has the household open; opening a
  file someone else holds says so, and `--take-over` (or the notification) lets you proceed
  when they are done. Two people on the same Mac take turns; nothing syncs.
- **Who is looking?** The picker (title bar, or **Household ▸ Who is looking?…**) sets the
  viewer; every screen filters through that person's access policies (§7). The demo identifies
  people by choice, without a secret.
- Every created object gets an access policy with its creator as owner (F162): personal objects
  default to *Fully shared*, company accounts to *Shared summary* and excluded from household
  calculations (§8.5).
- Useful flags for scripts and tests: `--new`, `--sample`, `--household FILE`, `--as-of DATE`,
  `--viewer <person id>`, `--owner NAME`, `--take-over`.

## Privacy & authorization (M10, §7, §5.13–§5.18)

Ownership, visibility, calculation access and disclosure are four separate things; a missing
policy fails closed (F162). **Privacy** shows, filtered through the viewer's own policies:

- **Access policies** (§7.5): per object, who may learn it exists, see its balance, its
  transactions, forecasts, assumptions and explanations; use in calculations; disclosure of a
  restricted contribution; purposes; version history. Objects the viewer may not discover are
  not listed at all (V062). **Set policy…** writes a new, effective-dated version (owners only;
  `previous_versions` keep every earlier one so a historical calculation replays under the
  policy that governed it, V071; the forecast record pins policy versions).
- **Purpose-specific grants** (§7.4): one object, one person or role, one purpose (household
  forecasts, a scenario, decisions, funding searches, tax), an effective range. The engine
  evaluates authorization before an object becomes a forecast input or funding source; an
  account authorized only for one scenario is unavailable everywhere else (V067).
- **Difference-attack suppression** (§7.6, V073): in any projected chain, a single restricted
  contribution next to disclosed terms is not shown as "total − rest" — the breakdown collapses
  to the authorized total. The same rule projects the F139 attribution on the Scenarios screen.
- **Fail-closed checks** (V077): objects without a policy, conflicting policies and dangling
  grants, plus a sample denial message that names the object's kind and policy version, never
  the object.
- **Privacy audit log** (§5.18): policy changes with versions, grants, revocations, refused
  changes and viewer switches — immutable, saved with the household (`audit` table).

## Decisions (M9, §13, §19, §20, §26)

**Decisions** is a step-by-step builder, as asked: *what* (price, purchase date, purchase window,
reserve to keep, objective) → *down payment* (amount, grid range, the personal accounts in
funding order with optional floors, company salary routes) → *recurring payment* (annuity
months, rate, first instalment, paying account) → *other costs* (one-off and monthly running
costs) → **result**.

- The result shows the baseline-vs-decision graph, the §19.1 affordability metrics
  (immediate cash after purchase — with its chain — lowest cash and its date in the expected
  and conservative case, reserve remaining, future shortfalls, monthly repayment, financing
  cost, free cash flow, recovery time, taxes/fees triggered, goals delayed, company cash
  consequences).
- **Funding strategies** in the §13.5 format: every candidate with gross, net, fees,
  withholding, ending balances, reserve constraints, violations and caveats; gross-up finds the
  smallest gross that delivers the net after recomputed fees and withholding (M25, E02, V014);
  company routes carry the E07 ceiling and the M27 legal-capacity caveat and are rejected —
  never merely "cheapest tax" — when they break a reserve. The status always reads *best among
  the enumerated candidates*, never a global optimum.
- **Purchase month × down payment grid** (§19.2, E03) under the conservative case, the best
  cell under the objective marked; goal trade-offs (§20) from the household's goals; the
  conditional statement (§19.3) with every ranged assumption; the §26 recommendation contract.
- **Save as scenario** turns the plan into a scenario with its events, ready for the Scenarios
  comparison and the timeline.

## Scenarios (M8, §18)

A scenario is an **overlay** over the baseline: the series, funding rules and assumptions tagged
with it plus explicit, typed changes (§18.2 — end a stream, remove events, change an amount from
a date, move dates, add employment, add/remove a company, add/disable a tax rule). The engine
applies the overlay (`Household::apply_scenarios`) and runs unchanged on the result, so
forecasts, taxes, rules and the timeline all agree.

- **Composition** (§18.3): tick two or more scenarios; the compatibility check names any two
  changes that touch the same series, company or tax rule; **Compose…** creates a new scenario
  made of the members (private when any member is private).
- **Comparison** (§18.4): baseline versus the selection on household cash — balances on dates,
  lowest point and its date, reserve breaches, taxes and incremental taxes, fees, debt, cash
  runway (first passage, never "infinite"), company working capital and payroll coverage; a
  two-line chart; one named case at a time.
- **Difference attribution** (F139): every posting of the window belongs to exactly one bucket
  (a series, tax postings, fee events, starting cash), so the buckets sum to the end-of-window
  difference exactly — the screen shows the check.
- **Privacy** (§18.5): a private scenario, and any composition containing it, is listed and
  compared only for its owner.

## Rules (M7, §14)

**Rules** holds the household's deterministic rules: a scope (household, category, account,
person, company or scenario — more specific wins ties), a trigger (postings of a kind, or
funding searches), typed conditions (amount above/below, category, account, foreign
currency, date), one action (a percentage or fixed fee event, a classification, a funding
preference with a floor, a funding prohibition until a date, or bank selection with a
fallback), a priority, an effective range and a version history. Rules are saved with the
household (`rules` table; the tie-break policy in `meta`).

- The **conflict-resolution inspector** lists every decision the rules took in the window:
  every candidate, the outcome for each loser (lower priority / less specific / tie-break),
  and what resolved it (§14.7). The tie-break policy is explicit and persisted.
- **Fee events** become postings of their own right after the posting they belong to, enter
  the projection once and appear in its §2.1 chain as *Fees from user rules*; the forecast
  record names every rule that posted (`rules_applied`).
- **Funding order** (§14.5) shows what a funding search may use today, with floors and
  prohibitions; **bank selection** (§14.6) shows the preferred/fallback answer on today's
  balances. Scenario-scoped rules apply only inside their scenario (tick *Evaluate inside
  scenario*).
- **Simulate** runs the household forecast with and without one rule and shows the
  difference (§14.8) without applying anything. Enable/disable and priority changes record a
  new version; validation (V054) refuses inverted effective periods, fees above 100 %,
  unknown accounts and classification cycles by name.

## Monitoring

`atlas_app::alerting` logs every engine entry point and failure path and, when
`DEVBENCH_NOTIFY_URL` and `DEVBENCH_NOTIFY_TOKEN` are set in the deployment environment,
posts panics and calculation failures to DevBench's notify endpoint
(`POST $DEVBENCH_NOTIFY_URL/api/notify/`, bearer token, `{"text","level","source":"atlas-app"}`).
Without them it degrades to local logging and the status bar says "Alerts: log only".
The token is a server-side secret: it is read from the environment only and never logged.
