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
| `tools/gpui-shot` | `gpui-shot`, the headless screenshot helper the scripts below drive (Xvfb + lavapipe + X11 capture). A workspace member, never a dependency of the product. |
| `scripts/shoot.sh` | Headless screenshots of every screen, light and dark, plus sheets and details (Linux box). |
| `scripts/walkthrough.sh` | The captioned walkthrough video (`shots/walkthrough.mp4`) from a gpui-shot step sequence. |
| `scripts/perf-screens.sh` | Per-screen frame cost, before and after a change (`docs/perf.md`). |
| `scripts/setup-linux-sysroot.sh` | Vendors the system libraries gpui needs into `.sysroot` on a Linux box without root (once per box). |
| `docs/handover.md` | What was built per milestone, how to run and test, known limits. |
| `plan.md` | The requirements document this repo implements. |

## Build and run

```bash
cargo run --bin atlas -- --theme dark --sample --viewer a --screen today
cargo test                      # engine unit tests, E01–E08 examples, store, app UI integration tests
```

Options: `--theme light|dark`, `--size WxH`, `--screen <slug>` (see `atlas --help`),
`--viewer a|b` (which fixture person is looking — private objects project to aggregates),
`--perf-overlay` (show gpui's frame-time overlay, see below). With no household flag the app
opens on **Welcome**: create a household, open a file, or explore the fictitious sample.

## The screens

Eight destinations in the sidebar, each with its own tabs. Details are routes of their own and
carry the object's id, which a command line cannot name — so `--screen person`, `company`,
`account`, `rule` and `series-detail` open the **first** record of that kind the chosen viewer
may see, and the register instead when the household has none. A record hidden from the viewer
is never opened this way: `--screen` is a convenience, not a way past a policy.

| Destination | Tabs | Details |
|---|---|---|
| **Today** | — | the setup checklist, what is free now, the outlook and its assumptions |
| **Decisions** | Purchase · Scenarios · Extraction timing | the purchase result, a scenario comparison |
| **Forecast** | Path · Assumptions · Sensitivity | Derive from history, the read-only forecast record |
| **Accounts** | Accounts · Earmarks · Funding | one account (Overview, Earmarks, Planned movements, Properties) |
| **Activity** | Upcoming · Series · Actuals | one series; the occurrence and transaction inspectors |
| **People & companies** | People · Companies | one person, one company (full or planning-safe summary) |
| **Rules & taxes** | Rules · Rule activity · Taxes · Tax packs | one rule (Details, Decisions, Simulation, History), Create rule |
| **Sharing** | Policies · Grants · Audit | — |

Older slugs still work (`household`, `liquidity`, `timeline`, `projections`, `decisions`,
`privacy` …), so scripts and bookmarks from before the rebuild keep resolving.

### Logs and the frame meter (performance work)

Every run logs to **stderr and `logs.log`** in the current directory (`cargo run` → the
repository root; the previous run is kept as `logs.prev.log`). Send `logs.log` when the app
feels slow: it starts with the build profile (debug/release, opt-level), OS, window size and
scale factor, then every engine computation with its duration (`perf: compute … took 12.2ms`),
a summary line once a second while frames happen, and a line of its own for every slow frame
(over 50 ms; a hitch over 250 ms is a warning):

```text
perf: slow frame #12 section=timeline build=0.1ms draw≈95.3ms (layout=… prepaint=… paint=…) interval=101.0ms content(render=2.6ms) input(moves=3 wheel=0)
perf: summary 1.0s: 9 frames build avg=0.1ms max=0.4ms · draw≈ avg=64.7ms max=110.7ms · slow(>50ms)=5 … | gpui draw p50=71.4ms p90=113.1ms max=113.1ms n=9 · dirty→present p50=133.1ms …
```

Every frame's line exists too, at trace level — `ATLAS_LOG_FILE_FILTER=info,atlas_app=debug,atlas_app::perf=trace`
turns it on for a session that needs it (it is ~40 MB an hour while scrolling, which is why it
is off by default).

- `build` — time in the root view's render (title bar, status bar; small). `draw≈` — build +
  gpui layout + paint, measured to a probe painted last in the tree. The sidebar and the screen
  are cached views: gpui rebuilds them inside its `prepaint` phase only when they were notified,
  and a frame that reused the screen (a hover in the sidebar, typing in a dialog, a toast) says
  `content(cached)` instead of `content(render=…)`. `gpui draw` — gpui's own `Window::draw`
  histogram (the `profiler` feature), summarised once a second while frames happen.
- The **status bar** counter is gpui's own reading, refreshed once a second from its profiler
  histograms (`Window::frame_duration_snapshot`): `gpui: draw p50 6.3 ms · 58 fps · #50` — the
  median `Window::draw` time, and the frame rate gpui measured from the interval between
  presented frames while the window was animating. The rate reads `n/a (not animating)` when
  gpui presented no consecutive frames: it only draws when something changed, so a mouse
  crossing five buttons draws five frames and an idle window draws none. The green **overlay**
  in the top-right corner is gpui's other readout (current draw, 1 %/10 % worst, max, frame
  count), painted straight into the scene; it is off by default and `--perf-overlay` or
  `ATLAS_PERF_OVERLAY=1` shows it.
- `cargo run` compiles every dependency at opt-level 2 and `atlas-app` at 1 (see the profile
  notes in `Cargo.toml`); the engine crates stay unoptimised for debugging. The first build
  after pulling this is a full rebuild of the dependencies.
- Filters: `RUST_LOG=<filter>` (both sinks; the file keeps `atlas_*=debug` on top of it),
  `ATLAS_LOG_FILE_FILTER=<filter>` (file only), `ATLAS_LOG_FILE=<path>|off`.
- Module docs: `crates/atlas-app/src/perf.rs` and `logging.rs`. The investigation, the
  numbers and the **layout rules that keep taffy cheap** are in `docs/perf.md` — read it
  before writing a screen.

`--sample` loads the fictitious sample household (`atlas_core::fixtures`), whose numbers come
from the requirements' own worked examples. It is labelled *Fictitious sample* in the title
bar, and a real household never inherits its figures: a purchase draft, for instance, starts
empty rather than prefilled.

### Linux DevBench box

The box has no root and no GPU; everything the build and the screenshots need is in this
checkout. `source ~/.cargo/env` first, then once per box `scripts/setup-linux-sysroot.sh`: it
downloads the system libraries gpui links against (`apt-get download`, no root) into
`.sysroot/` (gitignored), which `.cargo/config.toml` hands to the linker and pkg-config. Keep
`~/.cargo/config.toml` at `jobs = 2` (2 GiB memory cgroup).

Screenshots: `scripts/shoot.sh [scenario]` builds `atlas` and `gpui-shot` (`tools/gpui-shot`,
a workspace member) and writes `shots/<name>.png` with the app log beside it; then
`devbench media put shots/<name>.png`. The helper starts its own Xvfb, hands the app Mesa's
lavapipe from `.sysroot` as its Vulkan driver, drives `--step click:X,Y | key:NAME | wait:MS |
shot:extra.png` before the capture, and exits 0 for a settled frame, 3 for a frame that never
settled or is blank, 1 on error — `target/debug/gpui-shot --help` has the rest. One-off:
`target/debug/gpui-shot --out shots/x.png -- target/debug/atlas --sample --viewer a --screen today`.
Its unit tests run with `cargo test -p gpui-shot`; `GPUI_SHOT_E2E=1 cargo test -p gpui-shot
--test e2e` drives a real window.

## Your data

Welcome offers **Create household…**, **Open household…** and **Explore the sample**;
**Household ▸ New household…** does the same later (name, base currency — USD by default —
reconciliation date, first person). Every register then has its own **Add …** command (people,
companies, accounts with their properties, planned movements with any supported recurrence,
assumptions, scenarios, transactions with matching, rules, tax rules), and a detail carries
the commands that belong to it — **Reconcile…**, **Change series…**, one-occurrence changes,
**Pay and release…**, **Delete …** with its consequence named.

- **Household ▸ Save as…** writes one portable SQLite file, `<name>.atlas.sqlite`, wherever
  you choose (default `~/Documents/Atlas/`). **Save** commits the relational household in one
  transaction after making a WAL-consistent snapshot in `backups/` (20 kept). Embedded,
  checksum-protected migrations upgrade older files before data is loaded. Open it again with
  **Household ▸ Open…** or `atlas --household /path/to/ours.atlas.sqlite`.
- **One editor at a time.** An atomically-created sidecar `.lock` file names who has the household open; opening a
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
- **Purpose-specific grants** (§7.4): one object, one person or a household role, one purpose
  (household forecasts, a scenario, decisions, funding searches, tax, taking money out of a
  company), an effective range. The engine
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
household (`rules` table; the tie-break policy in `households`).

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
Without them it degrades to local logging. Settings states which of the two is in force, under
Diagnostics.
The token is a server-side secret: it is read from the environment only and never logged.
