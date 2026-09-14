# Atlas Financer persistence

`atlas-store` owns every database and filesystem operation for portable
`*.atlas.sqlite` household files. `atlas-core` remains I/O-free and the app
continues to exchange only domain `Household` values with this crate.

## Runtime architecture

- SeaORM runs over a bounded SQLx SQLite pool.
- Every read/write connection enables WAL, foreign keys, a five-second busy
  timeout, and `synchronous=FULL`.
- `migrations/*.sql` is embedded in the binary and applied in filename order.
- `schema_migrations` records the version, application time, and SHA-256
  content checksum atomically with each migration.
- Opening a database with an unknown future migration or a changed checksum
  fails without writing it.
- A legacy `meta` plus JSON-table database is decoded, backed up with SQLite
  `VACUUM INTO`, migrated into a sibling temporary database, checked for exact
  domain equality and integrity, then atomically replaces the source.
- Saves, upgrades, overwrites, and restores preserve a consistent rolling
  backup before mutation. Twenty successful snapshots are retained.
- Sidecar locks are acquired with `create_new`, checked before writes, and
  released only by the owning process and owner identity.

Typed scalar fields, IDs, ordering, money, dates, and relationships are stored
relationally. JSON is retained only as a lossless domain payload or for bounded
Rust sum-type values. The relational projection is constrained and indexed;
the payload keeps forward migration independent from UI serialization details.

## Schema authoring

Tool versions are recorded in `TOOL_VERSIONS`. The normal workflow is:

```bash
crates/atlas-store/scripts/migrate-diff.sh add_descriptive_change
crates/atlas-store/scripts/migrate-lint.sh
crates/atlas-store/scripts/generate-entities.sh
```

The wrappers can be run from any directory. Contributors edit `schema.hcl`,
generate a timestamped migration, review the SQL, regenerate checked-in SeaORM
entities, update repository mappings and tests, and commit all related files.
Do not edit an applied migration, `atlas.sum`, or generated entity file.

`migrate-lint.sh` requires an Atlas login because current Atlas releases expose
migration linting through Atlas Pro. CI authenticates the pinned CLI with the
repository secret `ATLAS_CLOUD_TOKEN`; checksum validation, fresh-database
application, and schema drift checks do not use a household database.

Atlas CLI and `sea-orm-cli` are development tools. Neither is linked into or
distributed with Atlas Financer. Production files are migrated only by the
embedded Rust migrator.

## Recovery

Backups are in a `backups` directory beside the household file. A failed legacy
upgrade reports the exact preserved backup to the user. To restore, close other
editors, acquire the household lock, and call `HouseholdFile::restore_backup`.
Restore validates the snapshot first, backs up the current file, replaces it,
and leaves the caller holding the same lock.

Never copy only a live database's main file: WAL pages may contain committed
data. Use the store backup/restore API or close the application cleanly first.
