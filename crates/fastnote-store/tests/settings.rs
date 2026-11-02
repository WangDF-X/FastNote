use std::path::{Path, PathBuf};

use fastnote_core::{
    BallPosition, CloseBehavior, DateTime, DesktopSetting, DesktopSettings, EntryMode,
    SettingsRepository, Utc, WindowGeometry,
};
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    "2026-11-02T08:00:00Z".parse().unwrap()
}

fn database() -> (TempDir, PathBuf, SqliteSettings) {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake.db");
    let store = SqliteSettings::open(&path, now()).unwrap();
    (directory, path, store)
}

fn set(path: &Path, key: &str, value: &str) {
    Connection::open(path)
        .unwrap()
        .execute(
            "INSERT INTO app_settings(key, value, updated_at) VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value, now().to_rfc3339()],
        )
        .unwrap();
}

#[test]
fn test_fresh_settings_match_registered_defaults() {
    let (_directory, path, store) = database();
    assert_eq!(store.load_desktop().unwrap(), DesktopSettings::default());
    let size: String = Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT value FROM app_settings WHERE key='appearance.ball_size'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(size, "48");
}

#[test]
fn test_missing_keys_return_defaults_and_are_inserted_on_save() {
    let (_directory, path, mut store) = database();
    Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM app_settings", [])
        .unwrap();
    assert_eq!(store.load_desktop().unwrap(), DesktopSettings::default());
    store
        .save_desktop(&[DesktopSetting::EntryMode(EntryMode::Tray)], now())
        .unwrap();
    assert_eq!(store.load_desktop().unwrap().entry_mode, EntryMode::Tray);
}

#[test]
fn test_all_desktop_values_round_trip_and_can_clear_geometry() {
    let (_directory, path, mut store) = database();
    let geometry = WindowGeometry {
        x: -1800.0,
        y: 120.0,
        w: 1280.0,
        h: 800.0,
        maximized: true,
    };
    let position = BallPosition {
        x: -400.0,
        y: 200.0,
        monitor: "fake-screen".to_owned(),
    };
    store
        .save_desktop(
            &[
                DesktopSetting::EntryMode(EntryMode::Tray),
                DesktopSetting::CloseBehavior(CloseBehavior::Quit),
                DesktopSetting::MainGeometry(Some(geometry)),
                DesktopSetting::BallPosition(Some(position.clone())),
                DesktopSetting::FirstRunDone(true),
            ],
            now(),
        )
        .unwrap();
    let snapshot = store.load_desktop().unwrap();
    assert_eq!(snapshot.entry_mode, EntryMode::Tray);
    assert_eq!(snapshot.close_behavior, CloseBehavior::Quit);
    assert_eq!(snapshot.geometry, Some(geometry));
    assert_eq!(snapshot.ball_position, Some(position));
    assert!(snapshot.first_run_done);
    let updated: String = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT updated_at FROM app_settings WHERE key='window.main_geometry'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(updated, now().to_rfc3339());
    store
        .save_desktop(
            &[
                DesktopSetting::MainGeometry(None),
                DesktopSetting::BallPosition(None),
            ],
            now(),
        )
        .unwrap();
    assert_eq!(store.load_desktop().unwrap().geometry, None);
    assert_eq!(store.load_desktop().unwrap().ball_position, None);
    store.checkpoint().unwrap();
    drop(store);
    let store = SqliteSettings::open(&path, now()).unwrap();
    assert_eq!(
        store.load_desktop().unwrap().close_behavior,
        CloseBehavior::Quit
    );
}

#[test]
fn test_invalid_values_fall_back_without_changing_unknown_or_invalid_rows() {
    let (_directory, path, mut store) = database();
    for (key, value) in [
        ("general.entry_mode", "none"),
        ("general.close_behavior", "invalid"),
        ("hotkeys.enabled", "1"),
        ("general.first_run_done", "yes"),
        ("window.main_geometry", "{broken"),
        ("appearance.ball_position", "null"),
        ("future.fake", "preserve-me"),
    ] {
        set(&path, key, value);
    }
    assert_eq!(store.load_desktop().unwrap(), DesktopSettings::default());
    store
        .save_desktop(&[DesktopSetting::FirstRunDone(true)], now())
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    for (key, expected) in [
        ("future.fake", "preserve-me"),
        ("general.entry_mode", "none"),
    ] {
        let value: String = connection
            .query_row(
                "SELECT value FROM app_settings WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, expected);
    }
}

#[test]
fn test_old_geometry_missing_maximized_is_normal_and_extra_fields_are_compatible() {
    let (_directory, path, store) = database();
    set(
        &path,
        "window.main_geometry",
        r#"{"x":10,"y":20,"w":1280,"h":800,"future":true}"#,
    );
    assert!(!store.load_desktop().unwrap().geometry.unwrap().maximized);
    set(
        &path,
        "window.main_geometry",
        r#"{"x":10,"y":20,"w":10,"h":800}"#,
    );
    assert_eq!(store.load_desktop().unwrap().geometry, None);
    set(&path, "hotkeys.enabled", "false");
    set(&path, "hotkeys.show_app", "");
    let settings = store.load_desktop().unwrap();
    assert!(!settings.hotkeys_enabled);
    assert!(settings.show_hotkey.is_empty());
}

#[test]
fn test_duplicate_keys_and_invalid_geometry_reject_the_whole_batch() {
    let (_directory, _path, mut store) = database();
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::EntryMode(EntryMode::Tray),
                DesktopSetting::EntryMode(EntryMode::Ball),
            ],
            now()
        )
        .is_err());
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::FirstRunDone(true),
                DesktopSetting::MainGeometry(Some(WindowGeometry {
                    x: f64::NAN,
                    y: 0.0,
                    w: 1280.0,
                    h: 800.0,
                    maximized: false
                })),
            ],
            now()
        )
        .is_err());
    assert!(store
        .save_desktop(
            &[DesktopSetting::BallPosition(Some(BallPosition {
                x: f64::INFINITY,
                y: 0.0,
                monitor: "fake".to_owned(),
            }))],
            now()
        )
        .is_err());
    assert_eq!(store.load_desktop().unwrap(), DesktopSettings::default());
}

#[test]
fn test_constraint_failure_midway_rolls_back_earlier_settings() {
    let (_directory, path, mut store) = database();
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_fake_setting
        BEFORE UPDATE ON app_settings WHEN NEW.key='general.close_behavior'
        BEGIN SELECT RAISE(ABORT, 'fake test constraint'); END;",
        )
        .unwrap();
    let before = store.load_desktop().unwrap();
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::EntryMode(EntryMode::Tray),
                DesktopSetting::CloseBehavior(CloseBehavior::Quit),
            ],
            now()
        )
        .is_err());
    assert_eq!(store.load_desktop().unwrap(), before);
    store.save_desktop(&[], now()).unwrap();
}

#[test]
fn test_oversized_invalid_values_fall_back_without_reading_unknown_values() {
    let (_directory, path, store) = database();
    set(&path, "general.entry_mode", &"x".repeat(5000));
    set(&path, "hotkeys.show_app", "Ctrl+F01");
    set(&path, "future.fake_large", &"假内容".repeat(2000));
    assert_eq!(store.load_desktop().unwrap(), DesktopSettings::default());
}

#[test]
fn test_external_schema_upgrade_is_rejected_before_settings_write() {
    let (_directory, path, mut store) = database();
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let error = store
        .save_desktop(&[DesktopSetting::EntryMode(EntryMode::Tray)], now())
        .unwrap_err();
    assert!(matches!(
        error,
        fastnote_store::StoreError::NewerSchema { found: 99, .. }
    ));
    let entry: String = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT value FROM app_settings WHERE key='general.entry_mode'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(entry, "ball");
}

#[test]
fn test_checkpoint_lock_reports_error_but_committed_settings_remain() {
    let (_directory, path, mut store) = database();
    let reader = Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT value FROM app_settings;")
        .unwrap();
    store
        .save_desktop(&[DesktopSetting::EntryMode(EntryMode::Tray)], now())
        .unwrap();
    let error = store.checkpoint().unwrap_err();
    assert_eq!(error.code(), fastnote_core::StorageErrorCode::Locked);
    reader.execute_batch("ROLLBACK").unwrap();
    store.checkpoint().unwrap();
    assert_eq!(store.load_desktop().unwrap().entry_mode, EntryMode::Tray);
}
