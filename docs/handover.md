# Atlas Financer — handover (M0–M12 complete)

Repository branch `devbench/atlas-financer`, merge request against `main`:
https://github.com/mmujtabamain/atlas/pull/1. Everything below is on that branch.

## What was built

A Rust + gpui (gpui-kit 0.6) desktop application implementing `plan.md` (requirements v3.2)
as a **verbose, provenance-first UI**: every figure carries its §2.1 chain, its money class,
certainty and result strength; every forward-looking screen lists its assumptions; every domain
object has a screen; nothing is inferred and no AI sits in the calculation path.

| Milestone | Delivered |
|---|---|
| M0 Foundation | Workspace, integer-minor-unit `Money`, provenance graph with §2.1 rendering and M55 projection, household model + fixture from the plan's examples, app shell, explain sheet, alerting to DevBench. |
| M1 Entities | People, Companies (E07 ceiling, §8.7 planning-safe output), Accounts (full §7 table, balance definitions). |
| M2 Liquidity | Reservations §17 (nested vs disjoint, hard vs soft), household/person/company boundaries, M13 first-passage runway and K*, E01/E08. |
| M3 Timeline | Series with every recurrence, exceptions, amount changes, M05 clocks, statuses from actuals (§16), filters, scenario overlay, series editor. |
| M4 Projections | Chronological forecast per boundary with intraday ordering (V010), availability-dated inflows, named cases (§10.6), transfer points, forecast record with input hash. |
| M5 Assumptions | Register with freshness (F117), derived assumptions §10.7, one-at-a-time sensitivity §10.8 with the joint caveat (V043), conditional statement §10.1. |
| M6 Taxes | Effective-dated packs (§25), brackets (M23), withholding credit (M24), events with cash dates, attribution §12.6, reserve §12.4, E05, user rules (unverified). |
| M7 Rules | Deterministic rules (scope/trigger/typed conditions/action/priority/effective range/version), conflict inspector (§14.7), fee events in the forecast, funding order §14.5, bank selection §14.6, simulation §14.8, V054 validation. |
| M8 Scenarios | Overlays with explicit changes (§18.2), composition with compatibility check (§18.3), §18.4 metrics, F139 attribution verified to the minor unit, §18.5 privacy. |
| M9 Decisions | Step-by-step builder (purchase → down payment → recurring → other costs → result), gross-up (M25/E02), §13.5 strategies with E07/M27 caveats, E03 grid, goals §20, conditional result §19.3, §26 contract, save as scenario. |
| M10 Privacy | Purposes and grants (§7.4), effective-dated policy versions (V071), audit log (§5.18), §7.6 suppression (V073), fail-closed explanations (V077), Privacy screen. |
| M11 Verification | E01–E08 regression suite tagged with model ids, per-screen UI tests, screenshot script, walkthrough video, docs. |
| M12 Real data | SQLite household files with backups and a lock, New/Open/Save/Save as…, who-is-looking picker, data entry for every object, selectable currency (USD default). |

## How to run and test

```bash
cd atlas
cargo run --bin atlas                     # the sample household
cargo run --bin atlas -- --new            # an empty household (your data)
cargo test                                # 90 core + 8 examples + 5 store + 5 app + 18 UI tests
scripts/shoot.sh                          # every screen, light + dark (Linux box)
scripts/walkthrough.sh                    # shots/walkthrough.mp4
```

Test suite: `crates/atlas-core/src/*` unit tests (money, provenance, liquidity, timeline,
forecast, assumptions, sensitivity, tax, rules, scenario, decision, authz, risk),
`crates/atlas-core/tests/examples.rs` (E01–E08), `crates/atlas-store` (round trip, backups,
lock, schema), `crates/atlas-app/tests/ui.rs` (gpui-kit `test-support` integration tests, one or
more per screen, both viewers).

## Monitoring

`atlas-app::alerting` posts panics and engine failures to `$DEVBENCH_NOTIFY_URL/api/notify/`
with `Authorization: Bearer $DEVBENCH_NOTIFY_TOKEN` (source `atlas-app`), read from the
environment only; unset means local logging with a single warning. Every engine entry point
logs at `info` (boundary, horizon, viewer, policy versions) and failure paths at `error`.

## Known limits (deliberate)

- Native file dialogs could not be verified on the Linux box (no XDG portal); the path field
  fallback works everywhere. Please try `cargo run --bin atlas -- --new` and **Save as… ▸
  Browse…** on macOS (board task M12.7).
- The tax packs are fictitious DEMO packs; user rules stay labelled unverified (§12.7).
- Funding search enumerates configured orders and routes; it never claims a global optimum
  (§13.5). Company extraction routes carry the legal-capacity caveat (M27) and are never
  inferred from cash (E07).
- Multi-user means one SQLite file, one editor at a time (lock file), a who-is-looking picker
  without a secret — as agreed in the requirements Q&A.
- gpui-shot cannot scroll (XI2 valuators are not delivered), so long pages are photographed with a
  taller window.

## Where things are

- `README.md` — layout, build, data files, per-milestone feature notes.
- `docs/ui-implementation-plan.md` — the milestone plan (M0–M12) as executed.
- `docs/real-data-requirements.md` — the M12 requirements analysis and the answers received.
- `scripts/shoot.sh`, `scripts/walkthrough.sh` — reference screenshots and the walkthrough.
- DevBench board tasks #2717–#2816 carry per-task notes, screenshots and reasons.
