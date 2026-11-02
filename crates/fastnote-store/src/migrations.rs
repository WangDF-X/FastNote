use fastnote_core::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::database::check_integrity;
use crate::StoreError;

pub(crate) const CURRENT_VERSION: i64 = 2;
const SCRIPTS: [&str; 2] = [
    include_str!("../migrations/m0001_init.sql"),
    include_str!("../migrations/m0002_reminders.sql"),
];

const DEFAULTS: &[(&str, &str)] = &[
    ("general.entry_mode", "ball"),
    ("general.autostart", "false"),
    ("general.close_behavior", "entry"),
    ("general.first_run_done", "false"),
    ("hotkeys.enabled", "true"),
    ("hotkeys.show_app", "Ctrl+Alt+Space"),
    ("hotkeys.new_task", "Ctrl+Alt+N"),
    ("hotkeys.new_note", "Ctrl+Alt+M"),
    ("appearance.theme", "system"),
    ("appearance.density", "comfortable"),
    ("appearance.ball_size", "48"),
    ("appearance.ball_opacity", "90"),
    ("appearance.ball_position", ""),
    ("appearance.week_starts_on", "monday"),
    ("tasks.default_sort", "priority"),
    ("today.show_in_progress", "true"),
    ("today.show_waiting", "true"),
    ("today.show_unscheduled", "true"),
    ("today.show_completed", "true"),
    ("today.tomorrow_collapsed", "true"),
    ("data.backup_dir", ""),
    ("window.main_geometry", ""),
];

pub(crate) fn migrate(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError> {
    let timestamp = now.to_rfc3339();
    for (index, script) in SCRIPTS.iter().enumerate() {
        let version = i64::try_from(index + 1).map_err(|_| StoreError::integrity())?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current >= version {
            if current > CURRENT_VERSION {
                return Err(StoreError::newer(current, CURRENT_VERSION));
            }
            continue;
        }
        if current != version - 1 {
            return Err(StoreError::integrity());
        }
        tx.execute_batch(script)?;
        if version == 1 {
            for (key, value) in [
                ("app_id", "fastnote"),
                ("created_at", timestamp.as_str()),
                ("first_run_at", timestamp.as_str()),
                ("last_backup_at", ""),
                ("last_backup_path", ""),
                ("app_version", env!("CARGO_PKG_VERSION")),
            ] {
                tx.execute(
                    "INSERT INTO app_meta(key, value) VALUES (?1, ?2)",
                    params![key, value],
                )?;
            }
            for (key, value) in DEFAULTS {
                tx.execute(
                    "INSERT INTO app_settings(key, value, updated_at) VALUES (?1, ?2, ?3)",
                    params![key, value, timestamp],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at, app_version) VALUES (?1, ?2, ?3)",
            params![version, timestamp, env!("CARGO_PKG_VERSION")],
        )?;
        tx.pragma_update(None, "user_version", version)?;
        // ARCH-04 §8.1：完整性验证在提交前，失败连同 DDL 回滚。
        check_integrity(&tx, false)?;
        tx.commit()?;
        tracing::info!(schema_version = version, "database migrated");
    }
    Ok(())
}
