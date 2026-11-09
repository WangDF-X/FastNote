use fastnote_core::{NoteError, NoteId};
use rusqlite::{Connection, TransactionBehavior};

use crate::database::check_write_version;

use super::NoteStoreError;

const DROP_UNDO: &str = "DROP TABLE IF EXISTS temp.note_undo_deliveries;
    DROP TABLE IF EXISTS temp.note_undo_reminders;
    DROP TABLE IF EXISTS temp.note_undo_folders;
    DROP TABLE IF EXISTS temp.note_undo;";

pub(super) fn delete_folder(connection: &mut Connection, id: &str) -> Result<u32, NoteStoreError> {
    let temp_store: i32 = connection.query_row("PRAGMA temp_store", [], |row| row.get(0))?;
    if temp_store != 1 {
        connection.execute_batch("PRAGMA temp_store=FILE; PRAGMA temp.cache_size=-1024;")?;
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM note_folders WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(NoteError::NotFound.into());
    }
    tx.execute_batch(DROP_UNDO)?;
    tx.execute("CREATE TEMP TABLE note_undo_folders AS WITH RECURSIVE subtree(id) AS (SELECT id FROM note_folders WHERE id=?1 UNION SELECT f.id FROM note_folders f JOIN subtree s ON f.parent_id=s.id) SELECT f.* FROM note_folders f JOIN subtree s ON f.id=s.id", [id])?;
    tx.execute_batch("CREATE TEMP TABLE note_undo AS SELECT n.* FROM notes n JOIN temp.note_undo_folders f ON n.folder_id=f.id;
        CREATE TEMP TABLE note_undo_reminders AS SELECT r.* FROM reminders r JOIN temp.note_undo n ON n.id=r.note_id;
        CREATE TEMP TABLE note_undo_deliveries AS SELECT d.* FROM reminder_deliveries d JOIN temp.note_undo_reminders r ON r.id=d.reminder_id;")?;
    let count = tx.query_row("SELECT (SELECT count(*) FROM temp.note_undo_folders)+(SELECT count(*) FROM temp.note_undo)", [], |row| row.get(0))?;
    tx.execute("DELETE FROM note_folders WHERE id=?1", [id])?;
    tx.commit()?;
    Ok(count)
}

pub(super) fn delete(
    connection: &mut Connection,
    id: Option<NoteId>,
) -> Result<u32, NoteStoreError> {
    // D-NOTE-01：大批正文留在文件后备的 SQLite 临时库，限制页缓存，不搬进 Rust/UI。
    let temp_store: i32 = connection.query_row("PRAGMA temp_store", [], |row| row.get(0))?;
    if temp_store != 1 {
        connection.execute_batch("PRAGMA temp_store=FILE; PRAGMA temp.cache_size=-1024;")?;
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let key = id.map(|id| id.to_string());
    let count: u32 = tx.query_row(
        "SELECT count(*) FROM notes WHERE ?1 IS NULL OR id=?1",
        [&key],
        |row| row.get(0),
    )?;
    if count == 0 {
        if id.is_some() {
            return Err(NoteError::NotFound.into());
        }
        return Ok(0);
    }
    // DDL 也在事务内：备份/删除中途失败时原数据及上一撤销批次均不变。
    tx.execute_batch(DROP_UNDO)?;
    tx.execute(
        "CREATE TEMP TABLE note_undo AS SELECT * FROM notes WHERE ?1 IS NULL OR id=?1",
        [&key],
    )?;
    tx.execute_batch(
        "CREATE TEMP TABLE note_undo_reminders AS SELECT r.* FROM reminders r
        JOIN temp.note_undo n ON n.id=r.note_id;
        CREATE TEMP TABLE note_undo_deliveries AS SELECT d.* FROM reminder_deliveries d
        JOIN temp.note_undo_reminders r ON r.id=d.reminder_id;",
    )?;
    tx.execute("DELETE FROM notes WHERE ?1 IS NULL OR id=?1", [&key])?;
    tx.commit()?;
    Ok(count)
}

pub(super) fn restore(connection: &mut Connection) -> Result<u32, NoteStoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_temp_master WHERE name='note_undo')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(NoteError::UndoExpired.into());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let count: u32 = tx.query_row("SELECT count(*) FROM temp.note_undo", [], |row| row.get(0))?;
    // 恢复时名称可能已被其他文件或目录占用；只调整临时副本，失败时整体回滚。
    let mut names = std::collections::BTreeMap::<
        Option<String>,
        std::collections::BTreeSet<super::names::NameKey>,
    >::new();
    {
        let mut statement = tx.prepare(
            "SELECT folder_id,name FROM notes UNION ALL SELECT parent_id,name FROM note_folders",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (parent, name) = row?;
            names
                .entry(parent)
                .or_default()
                .insert(super::names::NameKey(name));
        }
    }
    let folders_exist: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_temp_master WHERE name='note_undo_folders')",
        [],
        |row| row.get(0),
    )?;
    let mut folder_count = 0u32;
    if folders_exist {
        // 根节点先恢复，后代按深度恢复，确保外键始终有效。
        let folders = {
            let mut statement = tx.prepare("WITH RECURSIVE ordered(id,depth) AS (SELECT id,0 FROM temp.note_undo_folders WHERE parent_id IS NULL OR parent_id NOT IN (SELECT id FROM temp.note_undo_folders) UNION ALL SELECT f.id,o.depth+1 FROM temp.note_undo_folders f JOIN ordered o ON f.parent_id=o.id) SELECT f.id,f.parent_id,f.name,f.created_at,f.updated_at FROM temp.note_undo_folders f JOIN ordered o ON o.id=f.id ORDER BY o.depth,f.id")?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let expected: u32 =
            tx.query_row("SELECT count(*) FROM temp.note_undo_folders", [], |row| {
                row.get(0)
            })?;
        if folders.len() != expected as usize {
            return Err(crate::StoreError::integrity().into());
        }
        for (id, parent, base, created, updated) in folders {
            let used = names.entry(parent.clone()).or_default();
            let mut candidate = base.clone();
            let mut number = 2u32;
            while used.contains(&super::names::NameKey(candidate.clone())) {
                candidate = super::names::numbered(&base, number);
                number = number
                    .checked_add(1)
                    .ok_or_else(crate::StoreError::integrity)?;
            }
            tx.execute("INSERT INTO note_folders(id,parent_id,name,created_at,updated_at) VALUES(?1,?2,?3,?4,?5)", rusqlite::params![id, parent, candidate, created, updated])?;
            used.insert(super::names::NameKey(candidate));
            folder_count += 1;
        }
    }
    {
        let mut statement =
            tx.prepare("SELECT id,name,folder_id FROM temp.note_undo ORDER BY created_at,id")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let base: String = row.get(1)?;
            let parent: Option<String> = row.get(2)?;
            let used = names.entry(parent).or_default();
            let mut candidate = base.clone();
            let mut number = 2u32;
            while used.contains(&super::names::NameKey(candidate.clone())) {
                candidate = super::names::numbered(&base, number);
                number = number
                    .checked_add(1)
                    .ok_or_else(crate::StoreError::integrity)?;
            }
            tx.execute(
                "UPDATE temp.note_undo SET name=?2 WHERE id=?1",
                rusqlite::params![id, candidate],
            )?;
            used.insert(super::names::NameKey(candidate));
        }
    }
    // 兼容旧数据库的关联列，但恢复时始终解除关联，不操作任务。
    tx.execute_batch(
        "INSERT INTO notes(id,content,converted_task_id,created_at,updated_at,name,folder_id)
        SELECT n.id,n.content,NULL,n.created_at,n.updated_at,n.name,n.folder_id FROM temp.note_undo n;
        INSERT INTO reminders SELECT * FROM temp.note_undo_reminders;
        INSERT INTO reminder_deliveries SELECT * FROM temp.note_undo_deliveries;",
    )?;
    tx.execute_batch(DROP_UNDO)?;
    tx.commit()?;
    Ok(count + folder_count)
}

pub(super) fn discard(connection: &Connection) -> Result<(), NoteStoreError> {
    connection.execute_batch(DROP_UNDO)?;
    Ok(())
}
