use fastnote_core::{derive::calendar::WeekStart, DesktopSetting, SettingsRepository, Utc};
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};

#[test]
fn test_week_start_persists_and_invalid_values_fall_back_without_rewrite() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = dir.path().join("fake-week.db");
    let now = "2026-11-07T12:00:00Z"
        .parse::<chrono::DateTime<Utc>>()
        .unwrap();
    let mut store = SqliteSettings::open(&path, now).unwrap();
    assert_eq!(store.load_desktop().unwrap().week_start, WeekStart::Monday);
    store
        .save_desktop(&[DesktopSetting::WeekStart(WeekStart::Sunday)], now)
        .unwrap();
    drop(store);
    let store = SqliteSettings::open(&path, now).unwrap();
    assert_eq!(store.load_desktop().unwrap().week_start, WeekStart::Sunday);
    let db = Connection::open(&path).unwrap();
    db.execute(
        "UPDATE app_settings SET value=?1 WHERE key='appearance.week_starts_on'",
        params!["invalid"],
    )
    .unwrap();
    assert_eq!(store.load_desktop().unwrap().week_start, WeekStart::Monday);
    let stored: String = db
        .query_row(
            "SELECT value FROM app_settings WHERE key='appearance.week_starts_on'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, "invalid");
}
