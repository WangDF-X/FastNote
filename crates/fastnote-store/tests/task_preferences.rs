use std::path::PathBuf;

use fastnote_core::{
    DateTime, DesktopSetting, SettingsRepository, Task, TaskId, TaskRepository, TodayPreferences,
    TodaySort, Utc,
};
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    "2026-11-03T12:00:00Z".parse().unwrap()
}

fn database() -> (TempDir, PathBuf, SqliteSettings) {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake.db");
    let store = SqliteSettings::open(&path, now()).unwrap();
    (directory, path, store)
}

fn value(connection: &Connection, key: &str) -> String {
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .unwrap()
}

fn changes() -> Vec<DesktopSetting> {
    vec![
        DesktopSetting::TodaySort(TodaySort::DueDate),
        DesktopSetting::TodayShowUnscheduled(false),
        DesktopSetting::TodayShowCompleted(false),
    ]
}

#[test]
fn test_fresh_and_saved_preferences_match_settings_and_task_query_snapshots_after_reopen() {
    let (_directory, path, mut store) = database();
    assert_eq!(
        store.load_desktop().unwrap().today_preferences,
        TodayPreferences::default()
    );
    for sort in [
        TodaySort::Priority,
        TodaySort::DueDate,
        TodaySort::CreatedDescending,
    ] {
        for shown in [false, true] {
            store
                .save_desktop(
                    &[
                        DesktopSetting::TodaySort(sort),
                        DesktopSetting::TodayShowUnscheduled(shown),
                        DesktopSetting::TodayShowCompleted(shown),
                    ],
                    now(),
                )
                .unwrap();
            let preferences = store.load_desktop().unwrap().today_preferences;
            assert_eq!(preferences.sort, sort);
            assert_eq!(preferences.show_unscheduled, shown);
            assert_eq!(preferences.show_completed, shown);
            assert!(preferences.later_collapsed);
            assert_eq!(store.today_preferences().unwrap(), preferences);
        }
    }
    let connection = Connection::open(&path).unwrap();
    assert_eq!(value(&connection, "tasks.default_sort"), "created_desc");
    for key in ["today.show_unscheduled", "today.show_completed"] {
        assert_eq!(value(&connection, key), "true");
    }
    let before = store.load_desktop().unwrap();
    drop(connection);
    drop(store);
    let reopened = SqliteSettings::open(&path, now()).unwrap();
    assert_eq!(reopened.load_desktop().unwrap(), before);
    assert_eq!(
        reopened.today_preferences().unwrap(),
        before.today_preferences
    );
}

#[test]
fn test_missing_preference_keys_fall_back_and_each_supported_key_can_be_inserted() {
    let (_directory, path, mut store) = database();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "DELETE FROM app_settings WHERE key LIKE 'today.%' OR key='tasks.default_sort'",
            [],
        )
        .unwrap();
    assert_eq!(
        store.today_preferences().unwrap(),
        TodayPreferences::default()
    );
    assert_eq!(
        store.load_desktop().unwrap().today_preferences,
        TodayPreferences::default()
    );
    store.save_desktop(&changes(), now()).unwrap();
    assert_eq!(value(&connection, "tasks.default_sort"), "due_date");
    for key in ["today.show_unscheduled", "today.show_completed"] {
        assert_eq!(value(&connection, key), "false");
    }
    assert_eq!(
        store.today_preferences().unwrap(),
        store.load_desktop().unwrap().today_preferences
    );
}

#[test]
fn test_invalid_or_oversized_preferences_have_identical_fallback_without_overwriting_unrelated_keys(
) {
    let (_directory, path, mut store) = database();
    let connection = Connection::open(&path).unwrap();
    for (key, invalid) in [
        ("tasks.default_sort", "最近创建"),
        ("today.show_unscheduled", "yes"),
        ("today.show_completed", "1"),
        ("today.later_collapsed", "invalid"),
    ] {
        connection
            .execute(
                "UPDATE app_settings SET value=?2 WHERE key=?1",
                params![key, invalid],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO app_settings VALUES ('future.fake','preserve','fixed')",
            [],
        )
        .unwrap();
    assert_eq!(
        store.today_preferences().unwrap(),
        TodayPreferences::default()
    );
    assert_eq!(
        store.load_desktop().unwrap().today_preferences,
        TodayPreferences::default()
    );
    connection
        .execute(
            "UPDATE app_settings SET value=?1 WHERE key='tasks.default_sort'",
            ["x".repeat(5000)],
        )
        .unwrap();
    assert_eq!(
        store.load_desktop().unwrap().today_preferences,
        TodayPreferences::default()
    );
    store
        .save_desktop(&[DesktopSetting::TodayShowUnscheduled(false)], now())
        .unwrap();
    assert_eq!(value(&connection, "future.fake"), "preserve");
    assert_eq!(value(&connection, "today.later_collapsed"), "invalid");
    assert_eq!(value(&connection, "today.show_unscheduled"), "false");
}

#[test]
fn test_every_preference_constraint_failure_rolls_back_the_whole_batch() {
    for key in [
        "tasks.default_sort",
        "today.show_unscheduled",
        "today.show_completed",
    ] {
        let (_directory, path, mut store) = database();
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(&format!("CREATE TRIGGER reject_fake_preference BEFORE UPDATE ON app_settings WHEN NEW.key='{key}' BEGIN SELECT RAISE(ABORT,'fake constraint'); END;")).unwrap();
        let before = store.load_desktop().unwrap();
        assert!(store.save_desktop(&changes(), now()).is_err(), "{key}");
        assert_eq!(store.load_desktop().unwrap(), before);
        assert_eq!(store.today_preferences().unwrap(), before.today_preferences);
    }
}

#[test]
fn test_duplicate_keys_and_newer_schema_do_not_mutate_any_preference() {
    let (_directory, path, mut store) = database();
    let before = store.load_desktop().unwrap();
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::TodayShowCompleted(false),
                DesktopSetting::TodayShowCompleted(true)
            ],
            now()
        )
        .is_err());
    assert_eq!(store.load_desktop().unwrap(), before);
    let connection = Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(
        store.save_desktop(&changes(), now()),
        Err(fastnote_store::StoreError::NewerSchema { found: 99, .. })
    ));
    assert_eq!(value(&connection, "tasks.default_sort"), "priority");
    assert_eq!(value(&connection, "today.show_unscheduled"), "true");
}

#[test]
fn test_hiding_and_revealing_preferences_do_not_delete_or_modify_tasks_or_other_hidden_groups() {
    let (_directory, _path, mut store) = database();
    let id: TaskId = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    let task = Task::new(id, "假任务".to_owned(), now()).unwrap();
    store.insert_task(&task).unwrap();
    let before = store.task_detail(id).unwrap().unwrap();
    store
        .save_desktop(
            &[
                DesktopSetting::TodayShowUnscheduled(false),
                DesktopSetting::TodayShowCompleted(false),
            ],
            now(),
        )
        .unwrap();
    assert!(!store.today_preferences().unwrap().show_unscheduled);
    assert_eq!(store.task_detail(id).unwrap().unwrap(), before);
    store
        .save_desktop(&[DesktopSetting::TodayShowUnscheduled(true)], now())
        .unwrap();
    let restored = store.today_preferences().unwrap();
    assert!(restored.show_unscheduled);
    assert!(!restored.show_completed);
    assert_eq!(store.task_detail(id).unwrap().unwrap(), before);
}
