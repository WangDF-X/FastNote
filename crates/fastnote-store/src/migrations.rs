use fastnote_core::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::database::check_integrity;
use crate::StoreError;

pub(crate) const CURRENT_VERSION: i64 = 6;
const SCRIPTS: [&str; 6] = [
    include_str!("../migrations/m0001_init.sql"),
    include_str!("../migrations/m0002_reminders.sql"),
    include_str!("../migrations/m0003_task_times.sql"),
    include_str!("../migrations/m0004_task_workflow.sql"),
    include_str!("../migrations/m0005_note_files.sql"),
    include_str!("../migrations/m0006_journal_attachments.sql"),
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
    ("today.show_unscheduled", "true"),
    ("today.show_completed", "true"),
    ("today.later_collapsed", "true"),
    ("data.backup_dir", ""),
    ("window.main_geometry", ""),
];

pub(crate) fn migrate(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError> {
    let timestamp = now.to_rfc3339();
    for (index, script) in SCRIPTS.iter().enumerate() {
        let version = i64::try_from(index + 1).map_err(|_| StoreError::integrity())?;
        let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current > CURRENT_VERSION {
            return Err(StoreError::newer(current, CURRENT_VERSION));
        }
        if current >= version {
            continue;
        }
        migrate_step(connection, version, script, &timestamp)?;
        tracing::info!(schema_version = version, "database migrated");
    }
    Ok(())
}

fn migrate_step(
    connection: &mut Connection,
    version: i64,
    script: &str,
    timestamp: &str,
) -> Result<(), StoreError> {
    if !matches!(version, 4..=6) {
        return migrate_one(connection, version, script, timestamp);
    }
    // SQLite 表重建必须在事务外临时关闭外键；事务内仍执行 foreign_key_check。
    connection.pragma_update(None, "foreign_keys", false)?;
    let result = migrate_one(connection, version, script, timestamp);
    // 先求值恢复结果，即使迁移失败也恢复；migrate_one 的事务此时已经回滚/提交。
    let restored = connection.pragma_update(None, "foreign_keys", true);
    result?;
    restored?;
    Ok(())
}

fn migrate_one(
    connection: &mut Connection,
    version: i64,
    script: &str,
    timestamp: &str,
) -> Result<(), StoreError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current >= version {
        if current > CURRENT_VERSION {
            return Err(StoreError::newer(current, CURRENT_VERSION));
        }
        return Ok(());
    }
    if current != version - 1 {
        return Err(StoreError::integrity());
    }
    let task_objects = if version == 4 {
        let mut statement = tx.prepare(
            "SELECT name, sql FROM sqlite_schema WHERE tbl_name='tasks'
             AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name",
        )?;
        let objects = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        objects
    } else {
        Vec::new()
    };
    tx.execute_batch(script)?;
    if version == 5 {
        migrate_note_names(&tx)?;
    }
    for (name, sql) in task_objects {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name=?1)",
            [name],
            |row| row.get(0),
        )?;
        if !exists {
            tx.execute_batch(&sql)?;
        }
    }
    if version == 4 {
        // 缺失旧折叠键时使用已登记默认值及注入的迁移时刻，不读取系统时钟。
        tx.execute(
            "INSERT INTO app_settings(key,value,updated_at) VALUES('today.later_collapsed','true',?1)
             ON CONFLICT(key) DO NOTHING",
            [timestamp],
        )?;
    }
    if version == 1 {
        for (key, value) in [
            ("app_id", "fastnote"),
            ("created_at", timestamp),
            ("first_run_at", timestamp),
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
    Ok(())
}

// 在同一写事务中迁移旧名称；保留正文与时间，不将正文写入日志。
fn migrate_note_names(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    use crate::notes::names::NameKey;
    let mut statement = tx.prepare("SELECT id,content FROM notes ORDER BY created_at,id")?;
    let mut rows = statement.query([])?;
    let mut used = std::collections::BTreeSet::new();
    let mut next_numbers = std::collections::BTreeMap::<NameKey, u32>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let content: String = row.get(1)?;
        let base = crate::notes::names::capture_base(&content);
        let mut name = base.clone();
        let number = next_numbers.entry(NameKey(base.clone())).or_insert(2);
        while used.contains(&NameKey(name.clone())) {
            name = crate::notes::names::numbered(&base, *number);
            *number = number.checked_add(1).ok_or_else(StoreError::integrity)?;
        }
        tx.execute("UPDATE notes SET name=?2 WHERE id=?1", params![id, name])?;
        used.insert(NameKey(name));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
