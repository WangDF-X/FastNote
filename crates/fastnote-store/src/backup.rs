use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use fastnote_core::{DateTime, Utc};
use rusqlite::{
    backup::{Backup, StepResult},
    params, Connection, TransactionBehavior,
};
use serde_json::json;
use tempfile::TempPath;

use crate::database::check_integrity;
use crate::StoreError;

pub(crate) fn pre_migration_backup(
    source: &Connection,
    path: &Path,
    version: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let directory = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups");
    fs::create_dir_all(&directory)?;
    let stem = format!("pre-migration-v{version}-{}", now.format("%Y%m%d-%H%M%S"));
    let (temp, destination) = reserve_path(&directory, &stem)?;
    // TempPath 仅拥有本次 create_new 的半成品；失败自动清理，不碰已有备份。
    let temp = TempPath::try_from_path(temp)?;
    write_backup(source, &temp, version, now)?;
    finish_backup(temp, &destination)
}

fn finish_backup(temp: TempPath, destination: &Path) -> Result<(), StoreError> {
    // Windows 使用不覆盖目标的原子重命名，防止检查后出现同名文件的竞态。
    temp.persist_noclobber(destination)
        .map_err(|error| StoreError::file(error.error))
}

fn reserve_path(directory: &Path, stem: &str) -> Result<(PathBuf, PathBuf), StoreError> {
    for suffix in 1..=1000 {
        let name = if suffix == 1 {
            stem.to_owned()
        } else {
            format!("{stem}-{suffix}")
        };
        let destination = directory.join(format!("{name}.fnsbackup"));
        if destination.try_exists()? {
            continue;
        }
        let temp = directory.join(format!("{name}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => {
                drop(file);
                return Ok((temp, destination));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(StoreError::file(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "pre-migration backup name unavailable",
    )))
}

fn write_backup(
    source: &Connection,
    temp: &Path,
    version: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut target = Connection::open(temp)?;
    {
        let backup = Backup::new(source, &mut target)?;
        if !matches!(backup.step(-1)?, StepResult::Done) {
            return Err(StoreError::sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                None,
            )));
        }
    }
    target.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
    check_integrity(&target, true)?;
    let count = |table: &str| -> Result<i64, StoreError> {
        Ok(
            target.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })?,
        )
    };
    let counts = json!({
        "plans": count("plans")?, "tasks": count("tasks")?,
        "task_dependencies": count("task_dependencies")?, "notes": count("notes")?,
        "journals": count("journal_entries")?,
        "journal_attachments": if version >= 6 { count("journal_attachments")? } else { 0 },
        "reminders": if version >= 2 { count("reminders")? } else { 0 },
        "reminder_deliveries": if version >= 2 { count("reminder_deliveries")? } else { 0 },
    })
    .to_string();
    let tx = target.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (key, value) in [
        ("backup_created_at", now.to_rfc3339()),
        ("backup_app_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("backup_schema_version", version.to_string()),
        ("backup_counts", counts),
    ] {
        tx.execute(
            "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    tx.commit()?;
    check_integrity(&target, true)?;
    target
        .close()
        .map_err(|(_, error)| StoreError::from(error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn test_backup_rename_does_not_clobber_a_destination_created_after_reservation() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let mut temp = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        temp.write_all(b"fake backup").unwrap();
        let temp = temp.into_temp_path();
        let source = temp.to_path_buf();
        let destination = directory.path().join("fake.fnsbackup");
        fs::write(&destination, b"fake preexisting backup").unwrap();
        assert!(finish_backup(temp, &destination).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"fake preexisting backup");
        assert!(!source.exists());
    }
}
