use std::path::{Path, PathBuf};

use fastnote_core::{CloseBehavior, DateTime, DesktopSetting, SettingsRepository, ThemeMode, Utc};
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    "2026-11-02T08:00:00Z".parse().unwrap()
}

fn database() -> (TempDir, PathBuf, SqliteSettings) {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake-theme.db");
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

fn value(path: &Path, key: &str) -> String {
    Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT value FROM app_settings WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn test_every_theme_mode_round_trips_after_database_reopen_without_schema_change() {
    let (_directory, path, mut store) = database();
    for mode in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System] {
        assert_eq!(DesktopSetting::Theme(mode).key(), "appearance.theme");
        store
            .save_desktop(&[DesktopSetting::Theme(mode)], now())
            .unwrap();
        store.checkpoint().unwrap();
        drop(store);
        store = SqliteSettings::open(&path, now()).unwrap();
        assert_eq!(store.load_desktop().unwrap().theme, mode);
        assert_eq!(value(&path, "appearance.theme"), mode.as_str());
        let version: i64 = Connection::open(&path)
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }
}

#[test]
fn test_missing_theme_defaults_to_system_and_save_recreates_only_the_canonical_key() {
    let (_directory, path, mut store) = database();
    Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM app_settings WHERE key='appearance.theme'", [])
        .unwrap();
    set(&path, "general.theme_mode", "dark");
    assert_eq!(store.load_desktop().unwrap().theme, ThemeMode::System);
    store
        .save_desktop(&[DesktopSetting::Theme(ThemeMode::Light)], now())
        .unwrap();
    assert_eq!(store.load_desktop().unwrap().theme, ThemeMode::Light);
    assert_eq!(value(&path, "appearance.theme"), "light");
    // 未登记旧键视作未知键，不猜测迁移、覆盖或删除用户数据。
    assert_eq!(value(&path, "general.theme_mode"), "dark");
}

#[test]
fn test_invalid_or_oversized_theme_defaults_to_system_without_rewriting_the_row() {
    let (_directory, path, store) = database();
    for invalid in ["auto", "Dark", "", "true", "\"dark\"", "dark\0"] {
        set(&path, "appearance.theme", invalid);
        assert_eq!(store.load_desktop().unwrap().theme, ThemeMode::System);
        assert_eq!(value(&path, "appearance.theme"), invalid);
    }
    let large = "x".repeat(5000);
    set(&path, "appearance.theme", &large);
    assert_eq!(store.load_desktop().unwrap().theme, ThemeMode::System);
    assert_eq!(value(&path, "appearance.theme"), large);
}

#[test]
fn test_duplicate_theme_changes_and_mid_transaction_constraint_failure_are_atomic() {
    let (_directory, path, mut store) = database();
    let before = store.load_desktop().unwrap();
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::Theme(ThemeMode::Dark),
                DesktopSetting::Theme(ThemeMode::Light),
            ],
            now()
        )
        .is_err());
    assert_eq!(store.load_desktop().unwrap(), before);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_fake_theme_batch BEFORE UPDATE ON app_settings
         WHEN NEW.key='general.close_behavior'
         BEGIN SELECT RAISE(ABORT, 'fake test constraint'); END;",
        )
        .unwrap();
    assert!(store
        .save_desktop(
            &[
                DesktopSetting::Theme(ThemeMode::Dark),
                DesktopSetting::CloseBehavior(CloseBehavior::Quit),
            ],
            now()
        )
        .is_err());
    assert_eq!(store.load_desktop().unwrap(), before);
    assert_eq!(value(&path, "appearance.theme"), "system");
}
