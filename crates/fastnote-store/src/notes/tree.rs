//! NOTE-03 文件树操作；检查和写入均处于同一个立即事务。
use fastnote_core::{domain::NoteTreeNode, DateTime, Note, NoteError, NoteFileEntry, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use super::{names::same_name, NoteStoreError};
use crate::{database::check_write_version, SqliteSettings};

fn entries(db: &Connection) -> Result<Vec<NoteFileEntry>, NoteStoreError> {
    let mut query = db.prepare("SELECT id,parent_id,name,1 FROM note_folders UNION ALL SELECT id,folder_id,name,0 FROM notes")?;
    let rows = query.query_map([], |row| {
        Ok(NoteFileEntry {
            id: row.get(0)?,
            parent: row.get(1)?,
            name: row.get(2)?,
            is_folder: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn validate(
    tree: &[NoteFileEntry],
    id: &str,
    parent: Option<&str>,
    name: &str,
) -> Result<(), NoteStoreError> {
    if fastnote_core::domain::validate_note_name(name).is_err() {
        return Err(NoteError::InvalidName.into());
    }
    if tree.iter().any(|entry| {
        entry.id != id && entry.parent.as_deref() == parent && same_name(&entry.name, name)
    }) {
        return Err(NoteError::NameExists.into());
    }
    let nodes: Vec<_> = tree
        .iter()
        .map(|entry| NoteTreeNode {
            id: entry.id.clone(),
            parent: entry.parent.clone(),
            is_folder: entry.is_folder,
        })
        .collect();
    fastnote_core::domain::validate_note_move(
        &nodes,
        &id.to_owned(),
        parent.map(str::to_owned).as_ref(),
    )
    .map_err(|_| NoteError::Conflict)?;
    Ok(())
}

fn tx(db: &mut Connection) -> Result<Transaction<'_>, NoteStoreError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    Ok(tx)
}

impl SqliteSettings {
    /// 读取完整目录树，不读取小记正文。
    pub fn note_file_tree(&self) -> Result<Vec<NoteFileEntry>, NoteStoreError> {
        entries(&self.connection)
    }

    /// 创建文件夹，标识由调用者生成，重复名称或非法层级会拒绝。
    pub fn create_note_folder(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), NoteStoreError> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| NoteError::Conflict)?;
        if id.get_version_num() != 7 || id.get_variant() != uuid::Variant::RFC4122 {
            return Err(NoteError::Conflict.into());
        }
        let id = id.to_string();
        let tx = tx(&mut self.connection)?;
        let mut tree = entries(&tx)?;
        if tree.iter().any(|entry| entry.id == id) {
            return Err(NoteError::Conflict.into());
        }
        tree.push(NoteFileEntry {
            id: id.clone(),
            parent: parent.map(str::to_owned),
            name: name.to_owned(),
            is_folder: true,
        });
        validate(&tree, &id, parent, name)?;
        tx.execute("INSERT INTO note_folders(id,parent_id,name,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)", params![id, parent, name, now.to_rfc3339()])?;
        tx.commit()?;
        Ok(())
    }

    /// 在指定文件夹创建空白或已有内容的小记。
    pub fn create_note_file(
        &mut self,
        note: &Note,
        parent: Option<&str>,
        name: &str,
    ) -> Result<(), NoteStoreError> {
        fastnote_core::validate_note_content(&note.content)?;
        let tx = tx(&mut self.connection)?;
        let mut tree = entries(&tx)?;
        let id = note.id.to_string();
        if tree.iter().any(|entry| entry.id == id) {
            return Err(NoteError::Conflict.into());
        }
        tree.push(NoteFileEntry {
            id: id.clone(),
            parent: parent.map(str::to_owned),
            name: name.to_owned(),
            is_folder: false,
        });
        validate(&tree, &id, parent, name)?;
        tx.execute("INSERT INTO notes(id,folder_id,name,content,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)", params![id, parent, name, note.content, note.created_at.to_rfc3339(), note.updated_at.to_rfc3339()])?;
        tx.commit()?;
        Ok(())
    }

    /// 原子地重命名或移动文件／文件夹；不会覆盖同名节点。
    pub fn relocate_note_entry(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), NoteStoreError> {
        let tx = tx(&mut self.connection)?;
        let tree = entries(&tx)?;
        let entry = tree
            .iter()
            .find(|entry| entry.id == id)
            .ok_or(NoteError::NotFound)?;
        validate(&tree, id, parent, name)?;
        if entry.name != name || entry.parent.as_deref() != parent {
            if entry.is_folder {
                tx.execute(
                    "UPDATE note_folders SET parent_id=?2,name=?3,updated_at=?4 WHERE id=?1",
                    params![id, parent, name, now.to_rfc3339()],
                )?;
            } else {
                let old: String = tx
                    .query_row("SELECT updated_at FROM notes WHERE id=?1", [id], |row| {
                        row.get(0)
                    })
                    .optional()?
                    .ok_or(NoteError::NotFound)?;
                let old = DateTime::parse_from_rfc3339(&old)
                    .map_err(|_| crate::StoreError::integrity())?
                    .with_timezone(&Utc);
                let timestamp = now.max(old + chrono::Duration::microseconds(1));
                tx.execute(
                    "UPDATE notes SET folder_id=?2,name=?3,updated_at=?4 WHERE id=?1",
                    params![id, parent, name, timestamp.to_rfc3339()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
