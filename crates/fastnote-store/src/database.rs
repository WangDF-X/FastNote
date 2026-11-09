use std::fs;
use std::path::Path;
use std::time::Duration;

use fastnote_core::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::backup::pre_migration_backup;
use crate::migrations::{migrate, CURRENT_VERSION};
use crate::StoreError;

pub(crate) fn open(path: &Path, now: DateTime<Utc>) -> Result<Connection, StoreError> {
    // 更高版本/损坏库只用只读连接检查，连 journal_mode 都不能修改。
    if path.try_exists()? {
        let probe = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        inspect(&probe)?;
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let version = inspect(&connection)?;
    if version > 0 && version < CURRENT_VERSION {
        pre_migration_backup(&connection, path, version, now)?;
    }
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
        PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY; PRAGMA wal_autocheckpoint=1000;",
    )?;
    migrate(&mut connection, now)?;
    inspect(&connection)?;
    Ok(connection)
}

fn inspect(connection: &Connection) -> Result<i64, StoreError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_VERSION {
        return Err(StoreError::newer(version, CURRENT_VERSION));
    }
    if version < 0 {
        return Err(StoreError::integrity());
    }
    if version == 0 {
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        return if count == 0 {
            Ok(0)
        } else {
            Err(StoreError::integrity())
        };
    }
    let app_id: Option<String> = connection
        .query_row("SELECT value FROM app_meta WHERE key='app_id'", [], |row| {
            row.get(0)
        })
        .optional()?;
    let (count, min, max): (i64, i64, i64) = connection.query_row(
        "SELECT count(*), COALESCE(min(version), 0), COALESCE(max(version), 0) FROM schema_migrations",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if app_id.as_deref() != Some("fastnote") || count != version || min != 1 || max != version {
        return Err(StoreError::integrity());
    }
    for table in [
        "plans",
        "tasks",
        "task_dependencies",
        "notes",
        "journal_entries",
        "app_settings",
    ] {
        require_table(connection, table)?;
    }
    if version >= 2 {
        for table in ["reminders", "reminder_deliveries"] {
            require_table(connection, table)?;
        }
    }
    if version >= 5 {
        require_table(connection, "note_folders")?;
    }
    check_integrity(connection, false)?;
    Ok(version)
}

fn require_table(connection: &Connection, name: &str) -> Result<(), StoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StoreError::integrity())
    }
}

pub(crate) fn check_integrity(connection: &Connection, full: bool) -> Result<(), StoreError> {
    let sql = if full {
        "PRAGMA integrity_check"
    } else {
        "PRAGMA quick_check"
    };
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([])?;
    let Some(row) = rows.next()? else {
        return Err(StoreError::integrity());
    };
    if row.get::<_, String>(0)? != "ok" || rows.next()?.is_some() {
        return Err(StoreError::integrity());
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    if statement.query([])?.next()?.is_some() {
        return Err(StoreError::integrity());
    }
    Ok(())
}

pub(crate) fn check_write_version(connection: &Connection) -> Result<(), StoreError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_VERSION {
        return Err(StoreError::newer(version, CURRENT_VERSION));
    }
    if version != CURRENT_VERSION {
        return Err(StoreError::integrity());
    }
    Ok(())
}
