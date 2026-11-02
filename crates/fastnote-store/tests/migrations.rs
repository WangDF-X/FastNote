use std::fs;
use std::path::Path;

use fastnote_core::{DateTime, SettingsRepository, StorageErrorCode, Utc};
use fastnote_store::{SqliteSettings, StoreError};
use rusqlite::{params, Connection};

fn now() -> DateTime<Utc> {
    "2026-11-02T08:00:00Z".parse().unwrap()
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
}

fn version(connection: &Connection) -> i64 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn create_v1(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(include_str!("../migrations/m0001_init.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO app_meta(key, value) VALUES ('app_id', 'fastnote')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations VALUES (1, ?1, '0.0.1')",
            [now().to_rfc3339()],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    connection
        .execute(
            "INSERT INTO plans(id, name, created_at, updated_at) VALUES (?1, '假计划', ?2, ?2)",
            params!["019a0fff-aaaa-7000-8000-000000000001", now().to_rfc3339()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO app_settings VALUES ('general.entry_mode', 'tray', ?1)",
            [now().to_rfc3339()],
        )
        .unwrap();
}

#[test]
fn test_fresh_database_has_v2_schema_indexes_wal_and_audit() {
    let directory = tempdir();
    let path = directory.path().join("nested/fake.db");
    let store = SqliteSettings::open(&path, now()).unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(version(&connection), 2);
    let mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    for name in [
        "app_meta",
        "schema_migrations",
        "plans",
        "tasks",
        "task_dependencies",
        "notes",
        "journal_entries",
        "app_settings",
        "reminders",
        "reminder_deliveries",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
                [name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing table {name}");
    }
    for name in [
        "idx_tasks_plan",
        "idx_tasks_due",
        "idx_tasks_start",
        "idx_tasks_sort",
        "ux_notes_converted_task",
        "ux_reminders_task",
        "ux_reminders_note",
        "idx_reminders_next",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name=?1)",
                [name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing index {name}");
    }
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM schema_migrations", [], |row| row
                .get(0))
            .unwrap(),
        2
    );
    drop(connection);
    store.checkpoint().unwrap();
    drop(store);
    SqliteSettings::open(&path, now()).unwrap();
    assert!(!directory.path().join("backups").exists());
}

#[test]
fn test_v1_upgrade_preserves_data_and_produces_complete_v1_backup() {
    let directory = tempdir();
    let path = directory.path().join("fake.db");
    create_v1(&path);
    let store = SqliteSettings::open(&path, now()).unwrap();
    assert_eq!(store.load_desktop().unwrap().entry_mode.as_str(), "tray");
    let connection = Connection::open(&path).unwrap();
    assert_eq!(version(&connection), 2);
    let plan: String = connection
        .query_row("SELECT name FROM plans", [], |row| row.get(0))
        .unwrap();
    assert_eq!(plan, "假计划");
    let files: Vec<_> = fs::read_dir(directory.path().join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].extension().unwrap(), "fnsbackup");
    let backup = Connection::open(&files[0]).unwrap();
    assert_eq!(version(&backup), 1);
    let result: String = backup
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "ok");
    let counts: String = backup
        .query_row(
            "SELECT value FROM app_meta WHERE key='backup_counts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let counts: serde_json::Value = serde_json::from_str(&counts).unwrap();
    assert_eq!(counts["plans"], 1);
    assert_eq!(counts["reminders"], 0);
    let app: String = backup
        .query_row("SELECT value FROM app_meta WHERE key='app_id'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(app, "fastnote");
}

#[test]
fn test_failed_migration_rolls_back_ddl_and_does_not_overwrite_same_second_backup() {
    let directory = tempdir();
    let path = directory.path().join("fake.db");
    create_v1(&path);
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE reminder_deliveries(fake TEXT)")
        .unwrap();
    for _ in 0..2 {
        assert!(SqliteSettings::open(&path, now()).is_err());
    }
    let connection = Connection::open(&path).unwrap();
    assert_eq!(version(&connection), 1);
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM schema_migrations", [], |row| row
                .get(0))
            .unwrap(),
        1
    );
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='reminders')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!exists);
    assert_eq!(
        connection
            .query_row::<String, _, _>("SELECT name FROM plans", [], |row| row.get(0))
            .unwrap(),
        "假计划"
    );
    let files: Vec<_> = fs::read_dir(directory.path().join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .all(|path| path.extension().unwrap() == "fnsbackup"));
}

#[test]
fn test_backup_failure_prevents_any_schema_upgrade() {
    let directory = tempdir();
    let path = directory.path().join("fake.db");
    create_v1(&path);
    fs::write(directory.path().join("backups"), b"fake blocking file").unwrap();
    assert!(SqliteSettings::open(&path, now()).is_err());
    assert_eq!(version(&Connection::open(&path).unwrap()), 1);
}

#[test]
fn test_newer_schema_is_rejected_without_modifying_any_file() {
    let directory = tempdir();
    let path = directory.path().join("fake.db");
    {
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();
        connection
            .execute_batch("CREATE TABLE fake_future(value TEXT)")
            .unwrap();
    }
    let before = fs::read(&path).unwrap();
    let error = SqliteSettings::open(&path, now()).err().unwrap();
    assert!(matches!(error, StoreError::NewerSchema { found: 99, .. }));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn test_nonempty_unversioned_database_and_corrupt_file_are_not_rebuilt() {
    let directory = tempdir();
    for corrupt in [false, true] {
        let path = directory
            .path()
            .join(if corrupt { "corrupt.db" } else { "foreign.db" });
        if corrupt {
            fs::write(&path, b"fake corrupt SQLite bytes").unwrap();
        } else {
            Connection::open(&path)
                .unwrap()
                .execute_batch("CREATE TABLE fake_foreign(value TEXT)")
                .unwrap();
        }
        let before = fs::read(&path).unwrap();
        let error = SqliteSettings::open(&path, now()).err().unwrap();
        assert_eq!(error.code(), StorageErrorCode::Corrupt);
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn test_orphan_rows_and_wrong_app_identity_are_rejected_without_write() {
    let directory = tempdir();
    for wrong_app in [false, true] {
        let path = directory
            .path()
            .join(if wrong_app { "wrong.db" } else { "orphan.db" });
        create_v1(&path);
        {
            let connection = Connection::open(&path).unwrap();
            if wrong_app {
                connection
                    .execute(
                        "UPDATE app_meta SET value='fake-other' WHERE key='app_id'",
                        [],
                    )
                    .unwrap();
            } else {
                connection.execute_batch("PRAGMA foreign_keys=OFF;
                INSERT INTO task_dependencies VALUES ('fake-missing-a', 'fake-missing-b', '2026-11-02T08:00:00Z')").unwrap();
            }
        }
        let before = fs::read(&path).unwrap();
        assert!(SqliteSettings::open(&path, now()).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn test_inconsistent_migration_audit_is_rejected_before_any_file_change() {
    let directory = tempdir();
    let path = directory.path().join("fake.db");
    let store = SqliteSettings::open(&path, now()).unwrap();
    store.checkpoint().unwrap();
    drop(store);
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute("UPDATE schema_migrations SET version=0 WHERE version=1", [])
            .unwrap();
    }
    let before = fs::read(&path).unwrap();
    assert!(SqliteSettings::open(&path, now()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
