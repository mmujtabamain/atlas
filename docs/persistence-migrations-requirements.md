# Atlas Financer persistence and migrations requirements

Status: requirements and implementation contract

Scope: `atlas-store`, its integration with `atlas-app`, and all on-disk household data

Purpose: replace ad hoc schema creation and strict schema-version rejection with a durable, versioned SQLite persistence system

This document is self-contained. It defines the required architecture, tooling, safety properties, migration path, and acceptance criteria without relying on another repository or application.

## 1. Outcome

Atlas Financer must persist every user-owned household record in SQLite and must be able to open and upgrade every database version the product has released. Schema changes must be authored as immutable, ordered SQL migration files, checked by development tooling, embedded in the application, and applied automatically before any data is read or written.

The finished system must preserve the existing product guarantees:

- one portable `*.atlas.sqlite` file per household;
- one editor at a time, enforced by the existing sidecar lock workflow;
- a rolling backup made before any operation that can modify an existing household file;
- transactional writes with no observable partially saved household;
- deterministic save/load round trips and identical forecast inputs after reopening;
- user-facing New, Open, Save, Save As, sample, and recent-file behavior;
- no database or filesystem I/O in `atlas-core`;
- no personal or financial values in logs, telemetry, or failure alerts.

The database is the source of truth. The current pattern in which `save()` contains `CREATE TABLE IF NOT EXISTS` statements and rewrites loosely defined JSON tables is not an acceptable migration system.

## 2. Required technology and repository layout

### 2.1 Development-time schema tooling

Use Atlas CLI as a development-only schema migration authoring and validation tool.

Migration SQL must be generated automatically by Atlas from the difference between `schema.hcl` and the current migration head. Contributors update the desired schema and run the checked-in migration-diff wrapper; they must not normally hand-author schema migration SQL. The generated SQL must still be reviewed, linted, tested, and committed before use. Hand-written SQL is permitted only for data migrations or SQLite operations Atlas cannot express correctly, and the reason must be documented in that migration.

Required repository files:

```text
crates/atlas-store/
  atlas.hcl
  schema.hcl
  migrations/
    <UTC timestamp>_baseline.sql
    <UTC timestamp>_<change_name>.sql
    atlas.sum
  scripts/
    migrate-diff.sh
    migrate-lint.sh
    generate-entities.sh
  src/
    connection.rs
    entities/
    lib.rs
    migrations.rs
    repository/
```

The exact Rust module split may be refined during implementation, but schema definition, generated entities, migration application, connection setup, and domain mapping must remain distinct responsibilities.

`crates/atlas-store/atlas.hcl` must declare:

- `schema.hcl` as the desired schema;
- an in-memory SQLite development database;
- `migrations` as the migration directory;
- paths resolved relative to the persistence crate, keeping all Atlas-specific configuration self-contained in `atlas-store`.

Contributors must not need to remember Atlas configuration flags or working-directory rules. Checked-in wrapper scripts under `crates/atlas-store/scripts/` must locate the repository and persistence-crate directories themselves, pass the colocated configuration explicitly when needed, and provide a stable interface callable from any working directory.

The supported authoring loop must be documented and reproducible through those scripts. Conceptually it performs:

```bash
atlas migrate diff <descriptive_name> --env local
atlas migrate lint --env local --latest 1
atlas migrate hash --env local
```

The documentation must show the wrapper-script commands as the normal interface; the raw Atlas commands above describe the underlying operations only.

`migrate-diff.sh` must accept a descriptive migration name, invoke Atlas against the colocated desired schema and migration directory, produce the timestamped SQL file, refresh `atlas.sum`, and fail if Atlas reports an invalid or empty schema change. It must never apply the generated migration to a user's household database.

Applying migrations to a developer database may be supported for inspection and entity generation, but production household files must be migrated by application code, not by requiring users to install Atlas CLI.

### 2.2 Runtime database stack

Use a single async SQLite stack in `atlas-store`:

- SeaORM for typed entity access and ordinary queries;
- SQLx's SQLite pool underneath SeaORM so every connection receives the required SQLite options;
- `include_dir` or an equivalently deterministic compile-time mechanism to embed migration SQL in the application binary;
- `anyhow` internally where contextual errors are useful, mapped into stable `StoreError` variants at the crate boundary.

Do not ship Atlas CLI or `sea-orm-cli` with the application. They are development tools only.

Do not expose SeaORM entities outside `atlas-store`. `atlas-core` domain models and the UI-facing application model remain the product contract; explicit mapping functions translate between database entities and domain types.

### 2.3 Generated entities

Provide `crates/atlas-store/scripts/generate-entities.sh` that can be invoked from any working directory and:

1. creates a temporary SQLite database;
2. applies every checked-in migration to it;
3. regenerates SeaORM entities into `crates/atlas-store/src/entities`;
4. removes the temporary database, WAL, and shared-memory files on success or failure;
5. applies any documented, deterministic type normalization required by SQLite introspection;
6. fails if either Atlas CLI or `sea-orm-cli` is unavailable.

Generated entity code must be checked in so building Atlas Financer does not require either CLI. Regeneration must be deterministic, and CI must detect stale generated entities.

## 3. SQLite connection contract

Every read/write connection must be created through one shared connection factory. No feature may open SQLite directly with different settings.

Each connection must use:

- WAL journal mode;
- foreign-key enforcement enabled;
- a five-second busy timeout;
- a bounded pool sized for a desktop application;
- creation enabled only for explicit New or Save As flows;
- read/write mode for normal operation and genuinely read-only mode for non-mutating inspection where practical.

Connection initialization must surface contextual errors containing the household path but never record values from the household.

The application must not depend on process-global SQLite state. Foreign keys and other per-connection settings must be applied to every pooled connection.

## 4. Migration contract

### 4.1 Migration files

Every schema change must be a new SQL file named with a sortable UTC timestamp and a descriptive snake-case suffix. Migration files are append-only after release:

- never edit, rename, reorder, or delete an applied migration;
- never change `atlas.sum` manually;
- never put application startup schema creation in repository methods;
- never use the domain-model `SCHEMA_VERSION` constant as a substitute for migration history;
- keep data transformations deterministic and local to the migration that requires them.

The baseline migration must create the complete normalized schema, constraints, indexes, and metadata needed by a newly created household. Later changes must be represented only by later migrations.

### 4.2 Embedded runtime migrator

`atlas-store` must embed all `*.sql` files in the migration directory at compile time and apply them in filename order.

The runtime migrator must:

1. create a dedicated migration-history table if it does not exist;
2. read the set of already applied versions;
3. reject duplicate or malformed migration filenames;
4. apply only pending migrations;
5. run each migration atomically where SQLite permits;
6. record the migration version only after its SQL succeeds;
7. roll back both schema/data changes and the history insert if a migration fails;
8. stop immediately on failure and return the failing migration version with its error context;
9. be idempotent when called more than once;
10. run before repositories load any household data.

The migration-history table must minimally contain:

```sql
version TEXT PRIMARY KEY,
applied_at INTEGER NOT NULL
```

The implementation should also store or verify a content checksum for applied migrations. If an embedded migration's content no longer matches the recorded checksum, opening the file must fail safely rather than silently accepting rewritten history.

Migrations that require operations SQLite cannot perform safely inside the standard transaction wrapper, including foreign-key mode changes or table rebuilds, must use an explicitly tested migration path. They must run foreign-key integrity checks before commit and must never leave enforcement disabled for subsequent application work.

### 4.3 Upgrade policy

Opening an older supported household must upgrade it automatically after a backup succeeds. Opening a database whose migration history is newer than the application supports must fail read-only with a clear message that a newer Atlas Financer version is required.

A migration failure must never cause Atlas Financer to:

- continue with a partially understood schema;
- overwrite the source file with default or sample data;
- mark the failed migration as applied;
- delete the pre-migration backup;
- expose household contents in an alert or log.

There are no downgrade migrations. Recovery means reopening the preserved pre-migration backup with a compatible application version or upgrading with a corrected forward migration.

## 5. Schema and persistence model

### 5.1 Normalized relational schema

The target schema must represent persistent domain concepts as typed columns and relations rather than treating an entire entity as an opaque JSON blob. At minimum it must cover all currently persisted collections:

- household metadata;
- people;
- companies;
- accounts;
- reservations and earmarks;
- recurring series and their recurrence details, changes, and exceptions;
- assumptions;
- scenarios and scenario overlays;
- tax packs;
- policies and grants;
- actual transactions;
- reconciliation links;
- history records;
- rules and rule versions;
- goals and decisions;
- audit events.

The detailed schema must be derived from the domain model before the baseline is finalized. Repeated/nested structures must use child or join tables when they need querying, constraints, ownership, ordering, or independent evolution. JSON columns may remain only for bounded value objects where relational decomposition adds no integrity or query benefit; each retained JSON column must be documented.

### 5.2 Integrity requirements

The database must enforce, where SQLite can express them:

- primary keys for every entity;
- foreign keys for every stored relationship;
- explicit `ON DELETE` behavior matching domain rules;
- uniqueness for natural identifiers and deduplication keys where applicable;
- `NOT NULL` for required values;
- check constraints for closed enums, booleans, non-negative values, and valid ranges;
- stable ordering columns for user-ordered collections;
- indexes for foreign keys, common filters, ordering, reconciliation, and import deduplication;
- created and updated timestamps where the product needs auditability or conflict detection.

Money must be stored without floating-point loss, using the domain's canonical integer minor-unit representation plus currency where required. Dates and instants must have one documented encoding. IDs must retain their current stability and must never be silently regenerated during migration.

Database constraints complement domain validation; they do not replace it. Domain rules that span many rows or require authorization context remain enforced in `atlas-core`/the application service and are covered by tests.

### 5.3 Metadata

Product metadata and migration history are separate concerns. Household metadata must include at least:

- household name;
- base currency;
- reconciliation/as-of date;
- rule tie-break mode;
- last successful save time;
- application version that last wrote the file;
- a stable household identifier;
- the format lineage needed to recognize pre-migration legacy files.

The old single `schema_version` value may be retained temporarily only to recognize and import legacy files. New compatibility decisions must use migration history, not equality with one compile-time number.

## 6. Repository and transaction behavior

All persistence operations must live behind typed `atlas-store` APIs. UI code must not issue SQL or depend on generated entities.

Required behavior:

- load a household from a consistent database snapshot;
- save all changed data in one transaction;
- roll back the entire save if serialization, validation, SQL, or commit fails;
- avoid deleting and reinserting every table when a typed insert/update/delete can preserve row identity and reduce write amplification;
- prevent N+1 query patterns when loading related collections;
- preserve the domain's deterministic collection ordering;
- distinguish not found, validation, lock, incompatible-version, migration, corruption/integrity, serialization, SQLite, and filesystem failures;
- use parameterized queries only;
- keep migrations and ordinary repository operations independently testable against temporary databases.

All save and open work must remain off the GPUI rendering thread. Completion must return to the UI thread to update dirty state, notifications, current-file state, and recent files.

## 7. File lifecycle and data-loss protection

### 7.1 Creation and opening

Creating a household must:

1. validate and normalize the requested path;
2. refuse unintended overwrite;
3. create parent directories when appropriate;
4. create the SQLite file through the shared connection factory;
5. apply the full migration chain;
6. write the initial household in a transaction;
7. acquire or confirm the editor lock;
8. report success only after durable commit.

Opening a household must:

1. confirm that the path is a recognizable SQLite household file;
2. inspect compatibility without mutating it;
3. acquire the editor lock according to the existing owner/take-over workflow;
4. back up the file if migrations are pending;
5. apply pending migrations;
6. run integrity validation;
7. load the domain model;
8. replace in-memory application state only after every prior step succeeds.

Opening must not create a new empty database when the chosen file does not exist or is invalid.

### 7.2 Backups

Keep the existing rolling backup behavior, with 20 backups per household by default, and strengthen it for SQLite/WAL correctness.

- A backup is mandatory before migration and before overwriting an existing household through Save/Save As.
- Backups must be consistent SQLite snapshots. Do not use a plain filesystem copy of only the main database while uncheckpointed WAL data may exist; use SQLite's online backup API or a proven checkpoint-and-copy sequence.
- A backup must finish successfully before the protected mutation begins.
- Backup names must be collision-safe, sortable, and scoped to the source household.
- Pruning happens only after a new backup succeeds.
- A failed migration's backup must be retained regardless of normal pruning until the user has a recoverable source.
- Restore must be documented and covered by an automated test.

### 7.3 Locking and concurrent access

Preserve the sidecar lock file containing owner, acquisition time, and process ID. Make lock acquisition atomic so two processes cannot both believe they acquired an absent lock.

Before a write, verify that the current process still owns the lock. Keep the existing explicit take-over path, and never remove another owner's lock during ordinary close/release.

Also use SQLite transactions, WAL, busy timeout, and optimistic changed-on-disk detection. The sidecar communicates product ownership; SQLite locking protects database mechanics. Neither replaces the other.

Save As must acquire the destination lock and complete the destination write before releasing the source lock or switching the current-file state.

### 7.4 Durability and corruption handling

- Use a documented SQLite synchronous level appropriate for financial desktop data; default to `FULL` unless measured evidence justifies another choice.
- Run `PRAGMA quick_check` after migration and make `foreign_key_check` part of migration verification.
- Never auto-repair corruption by discarding rows.
- Offer errors that identify the file and recovery action without exposing its contents.
- Failed saves leave the in-memory household dirty and keep the last known-good on-disk database usable.
- Application shutdown must wait for or explicitly resolve an in-flight save and must release only locks owned by that application instance.

## 8. Legacy database migration

Existing `*.atlas.sqlite` files use a `meta` table plus ordered tables containing `id` and serialized JSON. They must be treated as user data, not disposable development fixtures.

Before changing runtime persistence, create fixture databases representing every legacy layout known to have existed, including:

- the earliest tables without later optional collections;
- the current 16-table JSON layout;
- empty households;
- realistic populated households with policies, links, rules, goals, grants, and audit events;
- a database with an unsupported future version;
- a malformed or partially corrupt database.

The legacy upgrade must:

1. detect the layout without relying only on a mutable metadata value;
2. create a consistent backup;
3. decode every legacy JSON row using the compatible legacy representation;
4. validate IDs, ordering, references, money, currencies, dates, policies, and required fields;
5. populate the normalized schema without replacing stable IDs;
6. compare record counts and domain-level invariants;
7. load the migrated household and prove it is equivalent to the pre-migration domain model;
8. prove the forecast input hash and key forecast results are unchanged;
9. record the appropriate migration history only after success;
10. leave the original database recoverable on any failure.

Do not silently default a legacy field merely because deserialization fails. Defaults are allowed only when the historical schema explicitly defined the field as absent and the intended value is documented and tested.

The implementation must choose and document one safe cutover mechanism: an in-place transactional table rebuild or migration into a new temporary SQLite database followed by an atomic replacement. For the current whole-household JSON format, a new database plus verified replacement is preferred because it makes rollback and cross-schema validation clearer.

## 9. Application integration requirements

The existing household lifecycle remains user-visible and must be rewired to the new store without behavioral regression:

- Welcome, New, Open, Open Sample, Save, Save As, Recent, and command-line household selection;
- unsaved-change prompts and dirty indicator;
- owner lock and explicit take-over messaging;
- background open/save with progress or disabled duplicate actions;
- viewer identity and authorization behavior;
- persisted settings required for household behavior;
- user-facing migration progress for upgrades that may take noticeable time;
- a clear recoverable error when upgrade fails, naming the preserved backup;
- recent-file state updated only after a successful open or save.

Samples and test fixtures must use the same repository and migration path as real households when persisted. No production code path may bypass migrations by constructing tables directly.

## 10. Observability and privacy

Structured logs may include:

- operation name;
- hashed or redacted path identity;
- migration version;
- duration;
- counts by entity type;
- success/failure classification;
- application version.

Logs and alerts must not include names, notes, descriptions, balances, transaction values, tax values, policy contents, raw database rows, serialized JSON, passcodes, or full filesystem paths containing personal names.

Migration, open, save, backup, restore, integrity, and lock failures must reach the existing production-failure alert path using sanitized context. Expected user decisions such as cancelling a file dialog are not failures.

## 11. Testing requirements

### 11.1 Migration tests

- empty database to migration head;
- every legacy fixture to migration head;
- each released migration boundary to migration head;
- repeated migrator invocation is a no-op;
- migration order is deterministic;
- checksum mismatch is rejected;
- failed SQL rolls back and is not recorded;
- unsupported future migration is rejected without mutation;
- foreign-key and quick integrity checks pass after every fixture upgrade;
- a fresh database generated from migrations matches `schema.hcl` with no diff.

### 11.2 Persistence tests

- empty and fully populated household round trips;
- domain equality after save/load;
- identical forecast input hash and key results after save/load and legacy migration;
- create, update, reorder, and delete for every persistent entity type;
- referential actions and refused deletes match domain behavior;
- transaction rollback leaves the prior database unchanged;
- concurrent reads and serialized writes behave under WAL and busy timeout;
- no N+1 regressions on household load;
- backup creation, retention, collision handling, restore, and WAL consistency;
- atomic lock acquisition, take-over, ownership verification, and release;
- Save As failure preserves the source file and source lock;
- corrupt and non-household files fail without overwrite.

### 11.3 Application integration tests

- New -> edit -> Save -> close -> Open reproduces the household;
- opening a legacy file upgrades it and shows the same product values;
- failed upgrade leaves current in-memory state untouched and presents recovery information;
- dirty state clears only after successful commit;
- duplicate Save/Open actions are prevented while I/O is in flight;
- viewer authorization remains unchanged after reopening;
- private data is absent from emitted logs and alert payloads.

Behavioral and visual verification remain a user-owned gate, but automated store and integration tests must make the data guarantees independently verifiable.

## 12. Developer workflow and CI gates

Document exact installation versions or minimum supported versions for Atlas CLI and `sea-orm-cli`. Pin or verify them in CI to avoid generated migration drift.

Required CI checks:

1. Atlas migration checksum validation;
2. Atlas migration lint;
3. apply all migrations to a fresh temporary SQLite database;
4. compare migration head with `schema.hcl` and require an empty diff;
5. regenerate SeaORM entities and require a clean working tree;
6. run `cargo fmt --check`;
7. run `cargo clippy` for the affected workspace crates with warnings denied;
8. run all `atlas-store` migration and persistence tests;
9. run affected `atlas-app` lifecycle integration tests.

A contributor changing persistent domain data must update, in the same change:

- `schema.hcl`;
- the Atlas-generated migration and refreshed `atlas.sum`;
- generated SeaORM entities when affected;
- repository/domain mappings;
- migration and round-trip fixtures/tests;
- this document or operational documentation if guarantees change.

## 13. Implementation sequence

Implementation must proceed in reviewable phases:

1. inventory the current domain model, persistent fields, relations, and all legacy layouts;
2. add pinned development tooling, schema definition, migration directory, and entity-generation script;
3. design and review the normalized schema before producing the baseline;
4. add the shared SQLx/SeaORM connection factory and embedded migrator;
5. add typed repositories and explicit entity/domain mappings;
6. add legacy fixtures and the verified legacy conversion path;
7. preserve and harden backups, locks, changed-on-disk checks, and restore behavior;
8. integrate background New/Open/Save/Save As lifecycle operations;
9. add CI drift, migration, integrity, round-trip, and privacy gates;
10. remove ad hoc `CREATE TABLE IF NOT EXISTS` and whole-table JSON rewrite code only after all compatibility tests pass.

At no point may an intermediate phase make existing household files unreadable without a tested upgrade and recovery path.

## 14. Definition of done

Persistence migration work is complete only when all of the following are true:

- a new household is created solely from checked-in migrations;
- an existing current-format household upgrades without losing or changing user-visible data;
- all known older fixtures upgrade to the same normalized migration head;
- pending migrations apply automatically before reads and writes;
- applied migrations are recorded atomically and protected against checksum drift;
- every connection enforces WAL, busy timeout, foreign keys, and the chosen durability setting;
- relational constraints and indexes cover the reviewed domain schema;
- backups are WAL-safe, restorable, retained, and created before upgrades;
- locking is atomic and verified before writes;
- save/load and migration preserve domain equality and forecast results;
- UI-thread blocking from persistence is eliminated;
- logs and alerts contain no household data;
- schema, migration, generated-entity, formatting, lint, and test gates pass;
- restoration and forward-migration failure handling are documented for users and maintainers;
- no production persistence path creates or mutates schema outside the migrator.

## 15. Explicit non-goals

This work does not add:

- a hosted synchronization server;
- simultaneous multi-writer collaboration;
- database downgrade migrations;
- encryption at rest unless separately specified;
- bank-specific import formats unrelated to persistence migration;
- changes to financial calculation rules or authorization semantics.

Those capabilities may build on this foundation later, but they must not weaken the migration, integrity, backup, locking, or privacy guarantees defined here.
