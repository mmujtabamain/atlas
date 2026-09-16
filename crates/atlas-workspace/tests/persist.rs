//! Files: atomic saves, recovery from corrupt or too-new files, the
//! migration chain, view-state scrubbing, and the saved-layouts directory.

mod common;

use atlas_workspace::persist::{self, LoadOutcome, PersistError};
use atlas_workspace::saved::{SavedError, SavedKind, SavedLayouts};
use atlas_workspace::{DockTarget, PaneDefinition, Preset, Scope, Side, WindowId, WorkspaceLayout};
use common::{assert_valid, json, open, picture, stack_of, three_columns_with_split};
use serde_json::{Value, json as j};
use std::fs;
use std::path::Path;

fn corrupt_copies(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap().to_string_lossy().contains(".corrupt-"))
        .collect();
    out.sort();
    out
}

#[test]
fn save_then_load_round_trips_and_keeps_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("layout.json");
    assert!(matches!(persist::load(&path), LoadOutcome::Fresh));

    let (ws, _) = three_columns_with_split();
    persist::save_atomic(&path, &ws).unwrap();
    assert!(path.exists());
    assert!(!persist::backup_path(&path).exists(), "no backup before a second save");
    assert!(!persist::temp_path(&path).exists(), "the temp file is renamed away");
    match persist::load(&path) {
        LoadOutcome::Loaded(loaded) => {
            assert_eq!(loaded, ws);
            assert_eq!(json(&loaded), json(&ws));
        }
        other => panic!("expected Loaded, got {other:?}"),
    }

    let mut changed = ws.clone();
    let p2 = changed.find_panes("two", None)[0].clone();
    changed.close_pane(&p2).unwrap();
    persist::save_atomic(&path, &changed).unwrap();
    let backup = persist::backup_path(&path);
    assert!(backup.exists(), ".bak exists after the second save");
    let backed: WorkspaceLayout = serde_json::from_slice(&fs::read(&backup).unwrap()).unwrap();
    assert_eq!(backed, ws, "the backup is the previous file");
    match persist::load(&path) {
        LoadOutcome::Loaded(loaded) => assert_eq!(loaded, changed),
        other => panic!("expected Loaded, got {other:?}"),
    }
}

#[test]
fn a_corrupt_file_is_kept_aside_and_a_default_returned() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    let garbage = b"{ this is not json";
    fs::write(&path, garbage).unwrap();
    match persist::load(&path) {
        LoadOutcome::Recovered { layout, diagnostics, reason } => {
            assert_valid(&layout);
            assert!(layout.is_empty(), "no previous copy: a fresh default");
            assert!(diagnostics.exists(), "{diagnostics:?}");
            assert!(diagnostics.to_string_lossy().contains("layout.json.corrupt-"));
            assert_eq!(fs::read(&diagnostics).unwrap(), garbage, "the original bytes are preserved");
            assert!(reason.contains("not valid JSON"), "{reason}");
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
    assert!(path.exists(), "the corrupt file itself is never deleted");
    assert_eq!(corrupt_copies(dir.path()).len(), 1);

    // A second recovery does not overwrite the first copy.
    match persist::load(&path) {
        LoadOutcome::Recovered { diagnostics, .. } => assert!(diagnostics.exists()),
        other => panic!("expected Recovered, got {other:?}"),
    }
    assert_eq!(corrupt_copies(dir.path()).len(), 2);
}

#[test]
fn recovery_prefers_the_previous_good_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    let (good, _) = three_columns_with_split();
    persist::save_atomic(&path, &good).unwrap();
    let mut later = good.clone();
    open(&mut later, "five", DockTarget::edge(Side::Left));
    persist::save_atomic(&path, &later).unwrap();
    fs::write(&path, b"\0\0\0").unwrap();
    match persist::load(&path) {
        LoadOutcome::Recovered { layout, .. } => assert_eq!(layout, good, "the .bak from before the last save comes back"),
        other => panic!("expected Recovered, got {other:?}"),
    }
}

#[test]
fn a_newer_schema_version_is_recovered_with_the_version_in_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    let mut document = serde_json::to_value(WorkspaceLayout::new("future")).unwrap();
    document["schemaVersion"] = j!(persist::SCHEMA_VERSION + 5);
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    match persist::load(&path) {
        LoadOutcome::Recovered { reason, diagnostics, .. } => {
            assert!(reason.contains(&(persist::SCHEMA_VERSION + 5).to_string()), "{reason}");
            assert!(reason.contains("newer"), "{reason}");
            assert!(diagnostics.exists());
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
}

#[test]
fn an_invalid_document_is_recovered_with_the_violations_in_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    let mut document = serde_json::to_value(three_columns_with_split().0).unwrap();
    // The same pane twice: nothing normalization can repair.
    document["windows"][0]["root"]["children"][0]["panes"] = j!(["pane_2"]);
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    match persist::load(&path) {
        LoadOutcome::Recovered { reason, .. } => {
            assert!(reason.contains("pane_2"), "{reason}");
            assert!(reason.contains("invariant"), "{reason}");
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
    let error = persist::migrate(document).unwrap_err();
    assert!(matches!(error, PersistError::Invalid(ref violations) if !violations.is_empty()), "{error}");
}

#[test]
fn missing_schema_version_is_read_as_version_one() {
    let mut document = serde_json::to_value(three_columns_with_split().0).unwrap();
    document.as_object_mut().unwrap().remove("schemaVersion");
    let loaded = persist::migrate(document.clone()).unwrap();
    assert_eq!(loaded.schema_version, persist::SCHEMA_VERSION);
    document["schemaVersion"] = j!(0);
    assert!(persist::migrate(document).is_ok());
}

#[test]
fn the_migration_chain_runs_synthetic_steps_in_order() {
    fn v1_to_v2(mut value: Value) -> Result<Value, PersistError> {
        value["name"] = j!(format!("{} (v2)", value["name"].as_str().unwrap_or("")));
        value["schemaVersion"] = j!(2);
        Ok(value)
    }
    fn v2_to_v3(mut value: Value) -> Result<Value, PersistError> {
        value["name"] = j!(format!("{} (v3)", value["name"].as_str().unwrap_or("")));
        value["schemaVersion"] = j!(3);
        Ok(value)
    }
    fn broken(value: Value) -> Result<Value, PersistError> {
        Ok(value) // forgets to bump the version
    }
    let chain: &[persist::Migrator] = &[v1_to_v2, v2_to_v3];
    let document = serde_json::to_value(three_columns_with_split().0).unwrap();

    let migrated = persist::migrate_with(document.clone(), chain, 3).unwrap();
    assert_eq!(migrated.name, "test (v2) (v3)");
    assert_eq!(migrated.schema_version, 3);
    assert_valid(&migrated);

    let mut already_v2 = document.clone();
    already_v2["schemaVersion"] = j!(2);
    assert_eq!(persist::migrate_with(already_v2, chain, 3).unwrap().name, "test (v3)");

    let error = persist::migrate_with(document.clone(), &[], 2).unwrap_err();
    assert!(matches!(error, PersistError::NoMigrator { from: 1, to: 2 }), "{error}");

    let error = persist::migrate_with(document.clone(), &[broken], 2).unwrap_err();
    assert!(matches!(error, PersistError::Migration { from: 1, .. }), "{error}");

    let mut future = document;
    future["schemaVersion"] = j!(4);
    let error = persist::migrate_with(future, chain, 3).unwrap_err();
    assert!(matches!(error, PersistError::Newer { found: 4, supported: 3 }), "{error}");
}

#[test]
fn scrubbing_removes_secrets_before_a_layout_is_saved() {
    let (mut ws, [p1, _, _, _]) = three_columns_with_split();
    ws.set_view_state(&p1, j!({ "scroll": 3, "token": "abc", "auth": { "secret": "x", "user": "me" }, "big": "y".repeat(20_000) }))
        .unwrap();
    assert!(!persist::is_persistable_view_state(&ws.pane(&p1).unwrap().view_state));
    let removed = persist::scrub_workspace(&mut ws);
    assert_eq!(removed, 3);
    assert_eq!(ws.pane(&p1).unwrap().view_state, j!({ "scroll": 3, "auth": { "user": "me" } }));
    assert!(persist::is_persistable_view_state(&ws.pane(&p1).unwrap().view_state));

    // Saved layouts scrub on the way in.
    let dir = tempfile::tempdir().unwrap();
    let mut saved = SavedLayouts::open(dir.path()).unwrap();
    ws.set_view_state(&p1, j!({ "password": "hunter2", "filter": "all" })).unwrap();
    let id = saved.save("Desk", &ws).unwrap();
    let stored = saved.get(&id).unwrap().layout.as_ref().unwrap();
    assert_eq!(stored.pane(&p1).unwrap().view_state, j!({ "filter": "all" }));
    let on_disk = fs::read_to_string(dir.path().join(format!("{id}.json"))).unwrap();
    assert!(!on_disk.contains("hunter2"));
}

// ---------------------------------------------------------------------------
// Saved layouts
// ---------------------------------------------------------------------------

#[test]
fn saved_layouts_round_trip_through_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let (ws, _) = three_columns_with_split();
    let household = ws.clone().with_scope(Scope::household("hh-1"));

    let mut saved = SavedLayouts::open(dir.path()).unwrap();
    assert!(saved.is_empty());
    let id = saved.save("Analysis desk", &household).unwrap();
    assert_eq!(saved.list().len(), 1);
    assert_eq!(saved.get(&id).unwrap().kind, SavedKind::Layout);
    assert_eq!(saved.get(&id).unwrap().scope, Scope::household("hh-1"));

    // Same name saves in place; save_as refuses the taken name.
    let again = saved.save("Analysis desk", &household).unwrap();
    assert_eq!(again, id);
    assert_eq!(saved.len(), 1);
    assert!(matches!(saved.save_as("Analysis desk", &household), Err(SavedError::NameTaken { .. })));
    assert!(matches!(saved.save_as("   ", &household), Err(SavedError::EmptyName)));

    let template_id = saved.save_template("Three up", &Preset::Analysis.template()).unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved.save_template("Analysis desk", &Preset::Compare.template()).is_ok(), "names are unique per kind, not across kinds");

    saved.rename(&id, "Analysis desk 2").unwrap();
    assert_eq!(saved.get(&id).unwrap().name, "Analysis desk 2");
    assert!(matches!(saved.rename(&id, "Analysis desk 2"), Ok(())), "renaming to its own name is fine");
    let copy = saved.duplicate(&id).unwrap();
    assert_ne!(copy, id);
    assert_eq!(saved.get(&copy).unwrap().name, "Analysis desk 2 (copy)");
    assert_eq!(saved.get(&copy).unwrap().layout, saved.get(&id).unwrap().layout);
    let copy2 = saved.duplicate(&id).unwrap();
    assert_eq!(saved.get(&copy2).unwrap().name, "Analysis desk 2 (copy 2)");

    // Reopening the directory sees the same entries.
    let reopened = SavedLayouts::open(dir.path()).unwrap();
    assert_eq!(reopened.len(), saved.len());
    assert_eq!(reopened.get(&id).unwrap(), saved.get(&id).unwrap());
    assert_eq!(
        reopened.list().iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(),
        vec!["Analysis desk 2", "Analysis desk 2 (copy 2)", "Analysis desk 2 (copy)", "Analysis desk", "Three up"],
        "layouts first, then templates, each by name"
    );

    saved.delete(&copy).unwrap();
    saved.delete(&copy2).unwrap();
    assert!(matches!(saved.delete(&copy), Err(SavedError::UnknownId(_))));
    assert!(!dir.path().join(format!("{copy}.json")).exists());
    assert_eq!(SavedLayouts::open(dir.path()).unwrap().len(), 3);

    // Scope rules on load.
    let loaded = saved.load_into(&id, &Scope::household("hh-1")).unwrap();
    assert_eq!(picture(&loaded, 3, 3), "123\n123\n124");
    assert_valid(&loaded);
    let error = saved.load_into(&id, &Scope::household("hh-2")).unwrap_err();
    assert!(matches!(error, SavedError::WrongHousehold { .. }), "{error}");
    assert!(matches!(saved.load_into(&id, &Scope::global()), Err(SavedError::WrongHousehold { .. })));
    let from_template = saved.load_into(&template_id, &Scope::household("hh-2")).unwrap();
    assert_eq!(from_template.scope, Scope::household("hh-2"));
    assert_eq!(picture(&from_template, 3, 3), "112\n113\n113");
    assert_eq!(from_template.panes.values().map(|d| d.kind.as_str()).collect::<Vec<_>>(), vec!["main", "supporting", "detail"]);
    assert_valid(&from_template);

    // A global layout loads into any household.
    let global_id = saved.save_as("Everyone", &ws).unwrap();
    let loaded = saved.load_into(&global_id, &Scope::household("hh-9")).unwrap();
    assert_eq!(loaded.scope, Scope::household("hh-9"));
}

#[test]
fn unreadable_entries_are_skipped_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("junk.json"), b"nope").unwrap();
    fs::write(dir.path().join("notes.txt"), b"ignored").unwrap();
    let mut saved = SavedLayouts::open(dir.path()).unwrap();
    assert!(saved.is_empty());
    saved.save("One", &WorkspaceLayout::new("one")).unwrap();
    assert!(dir.path().join("junk.json").exists());
    assert_eq!(SavedLayouts::open(dir.path()).unwrap().len(), 1);
}

#[test]
fn the_default_layout_is_valid_and_saves_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.json");
    let default = WorkspaceLayout::default();
    assert_valid(&default);
    persist::save_atomic(&path, &default).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"schemaVersion\": 1"));
    assert!(text.contains("\"window_main\""));
    let mut loaded = persist::load(&path).into_layout();
    let p = loaded.open_pane(&WindowId::main(), PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
    assert_eq!(stack_of(&loaded, &p), loaded.stack_of(&p).unwrap());
}
