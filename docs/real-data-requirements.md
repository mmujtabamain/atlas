# Atlas Financer — real-data demo: requirements to settle and the M12 plan

**Context:** Mujtaba asked that the finished product be a working demo several people can use on
*real, user-entered data*, not the fixture. This note lists the decisions we need from him, the
defaults we adopt on our own, and how the milestone plan absorbs the goal.

What the code does today: every model type already derives `Serialize/Deserialize`; nothing is
persisted (the app boots from `fixtures::plan_household()`); the only mutation APIs are
reservations, series edits and assumption acceptance; policies fail closed (F162), so any object
created without an `AccessPolicy` is hidden from everyone. gpui exposes native file dialogs
(`prompt_for_paths`) on macOS; on the Linux box they need a CLI/path fallback.

## A. Questions for Mujtaba

1. **How do real numbers get in?** (a) manual forms only · (b) manual + an Atlas CSV template for
   actual transactions (`date, account, amount, description`) · (c) manual + import of the bank's own
   CSV with column mapping (needs one anonymised statement per bank). **Recommended (b)**, upgraded to
   (c) if samples arrive. Mandatory fields: account = name, kind, owners summing to 100 %, opening
   balance + as-of date; event = name, in/out, amount (exact or range), recurrence, account,
   certainty; actual = date, account, signed amount.
2. **Where does the household live; how do two people share it?** (a) one `<name>.atlas.json` file
   in a folder of your choice (iCloud/Dropbox/network share), one editor at a time (lock file,
   changed-on-disk check, rolling backups) · (b) same as SQLite · (c) a small sync server (hosting,
   after the demo). **Recommended (a)**; default folder `~/Documents/Atlas/`. Follow-up: same Mac or
   two machines?
3. **How does a person identify themselves?** (a) "Who is looking?" picker, no secret · (b) picker +
   per-person passcode hashed in the file — privacy presets then really hold between two people on
   one Mac (§23) · (c) macOS user accounts mapped to people. **Recommended (b).** The file stays
   readable to anyone with file access unless we add encryption at rest (default: no).
4. **Currency of the demo household?** single base currency, no FX: EUR / PKR / USD / other.
   **Recommended:** single currency; EUR if it is your household, PKR if we keep the plan's numbers.
5. **Taxes: fictitious or yours?** (a) keep the labelled DEMO pack · (b) you supply your
   jurisdiction's table (type, brackets, thresholds, effective dates, official link) and we encode it
   as an *unverified* user pack (§12.7) · (c) no tax in the demo. **Recommended (a) + (b) if the
   table arrives.**
6. **Which decision does the demo end on, and whose numbers?** (a) buy a car — E03 affordability grid
   (§19) · (b) take 500,000 out of the company — salary vs dividend vs loan (E02/E07, §13) · (c)
   Person A leaves the job in March (§18) · (d) all three. **Recommended (d).** And: your own
   household (how many people / companies / accounts?) or an empty household plus a realistic sample?
7. **macOS packaging?** (a) `cargo run --release` from the repo · (b) unsigned `Atlas.app` via
   `cargo-bundle`, built on your Mac · (c) signed + notarized `.dmg` (Apple Developer ID).
   **Recommended (a)** for the demo, (b) as a script. Which macOS version / chip?
8. **§46 configuration:** default horizon 6 / 12 / 24 months from the reconciliation date
   (**recommended 12**, adjustable); new reservations default hard+disjoint (**recommended**) or
   soft; daily precision with explicit same-day ordering (§11.1) — confirm.

## B. Defaults we adopt unless told otherwise

- JSON via the existing serde derives, `schema_version: 1`, `Currency` serialized as `"EUR"`,
  `#[serde(default)]` on every later field, ids never reused; file I/O in the app layer,
  `atlas-core` stays I/O-free (`Household::to_json/from_json`).
- Data safety (§23): atomic save (temp + rename), rolling backups (keep 20), lock file with owner,
  changed-on-disk check, unsaved-changes prompt, autosave off.
- Authorization at creation (F162): every new object gets an `AccessPolicy` with the creator as
  owner and a preset dropdown (§7.2); defaults Fully shared + Full for household objects, Shared
  summary + Excluded for company accounts (§8.5/§8.7).
- Model rules: shares total 100 % (joint default 50/50); series/account currency = base currency;
  delete refused while referenced; company accounts excluded from the household; new series
  `UserEstimated`, expense order 10 / income 20, lags 0.
- Reconciliation: "Reconcile account" sets settled balance + `last_reconciled`; `as_of` = latest
  reconciliation; `history` for derived assumptions computed from reconciliation links.
- Import (F120): `SourceOfTruth::Imported`, dedupe on (account, date, amount, normalised
  description), per-row errors, nothing written on a failed preview.
- No probabilities, ranges only; daily precision; no FX.
- CLI: `--household <path>`, `--new`, `--sample`, `--viewer <person-id>` so tests and Linux
  screenshots never need the native dialog.
- Real personal data never appears in screenshots or unprotected media; save/load/import failures
  alert the team with a path hash, never contents.

## C. Impact on the milestones

M12 lands right after M5 so every later editor (tax packs, rules, scenarios, decisions) is built
on persisted, user-editable state:

| Milestone | Absorbs |
|---|---|
| M6 | tax pack editor persisted; jurisdiction configured per person/company; DEMO vs user-unverified labels |
| M7 | rules and versions persisted |
| M8 | scenario creation and overlay editing in the UI |
| M9 | Goal/Decision as a persisted entity with user-entered price/window/down payment/reserve |
| M10 | viewer switcher pulled forward into M12; policy editor builds on preset-at-creation; audit events persisted |
| M11 | regression on saved files (save → load → identical forecast hash); walkthrough from an empty household to the chosen decision; macOS check by Mujtaba |

**M12 — Real data: persistence, data entry & multi-user**

1. Persistence: JSON schema v1, `to_json/from_json`, atomic save, backups, lock file,
   `--household`; round-trip test proving identical forecasts; alerts on failure.
2. Household lifecycle UI: New (empty) / Open sample / Open… / Save / Save as… / Recent;
   onboarding; dirty indicator; native dialogs with a path-field fallback.
3. Identity & authorization at entry: who-is-looking picker, optional passcode, title-bar viewer
   switcher, auto-created policy with preset on every create.
4. Data-entry dialogs: people, companies, accounts (§7), reservations (edit/delete), event series
   (all recurrence variants, transfers, linked accounts), assumptions, scenarios; validation;
   referential integrity.
5. Actuals, reconciliation & CSV import: entry, per-account reconcile, link actual ↔ occurrence
   (V012), history from links, import with preview and dedupe.
6. Tests & monitoring: round-trip, schema, validation, import; UI new → add → save → reopen; viewer
   switch hides a Private account; alerts on every failure path.
7. macOS check, walkthrough, docs: `cargo run --release` on macOS, optional `cargo-bundle` script,
   recorded walkthrough from empty household to the decision, README "Your data".

Risks: native file dialogs cannot be verified on the Linux box; gpui-kit forms need their values in
separate entities and have no validation framework; shared-folder use is last-writer-wins with a
lock file; any creation path that forgets the policy makes the object vanish (one test per type);
`--viewer a|b` and the default horizon are hard-coded and must be generalised.
