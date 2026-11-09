use fastnote_core::{validate_note_content, Note, NoteDetail, NoteError};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use crate::database::check_write_version;

use super::{read, NoteStoreError};

pub(super) fn insert_named(
    connection: &mut Connection,
    note: &Note,
    name: &str,
) -> Result<(), NoteStoreError> {
    validate_note_content(&note.content)?;
    if fastnote_core::domain::validate_note_name(name).is_err() {
        return Err(NoteError::InvalidName.into());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let conflict = {
        let mut statement = tx.prepare("SELECT name FROM notes WHERE folder_id IS NULL UNION ALL SELECT name FROM note_folders WHERE parent_id IS NULL")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut conflict = false;
        for existing in rows {
            if super::names::same_name(&existing?, name) {
                conflict = true;
                break;
            }
        }
        conflict
    };
    if conflict {
        return Err(NoteError::NameExists.into());
    }
    tx.execute(
        "INSERT INTO notes(id,name,content,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
        params![
            note.id.to_string(),
            name,
            note.content,
            note.created_at.to_rfc3339()
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn insert(connection: &mut Connection, note: &Note) -> Result<(), NoteStoreError> {
    validate_note_content(&note.content)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let base = super::names::capture_base(&note.content);
    let mut used = std::collections::BTreeSet::new();
    {
        let mut statement = tx.prepare("SELECT name FROM notes WHERE folder_id IS NULL UNION ALL SELECT name FROM note_folders WHERE parent_id IS NULL")?;
        for row in statement.query_map([], |row| row.get::<_, String>(0))? {
            used.insert(super::names::NameKey(row?));
        }
    }
    let mut name = base.clone();
    let mut number = 2u32;
    while used.contains(&super::names::NameKey(name.clone())) {
        name = super::names::numbered(&base, number);
        number = number
            .checked_add(1)
            .ok_or_else(crate::StoreError::integrity)?;
    }
    tx.execute(
        "INSERT INTO notes(id,content,created_at,updated_at,name) VALUES(?1,?2,?3,?4,?5)",
        params![
            note.id.to_string(),
            note.content,
            note.created_at.to_rfc3339(),
            note.updated_at.to_rfc3339(),
            name
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn replace(
    connection: &mut Connection,
    expected: &Note,
    next: &Note,
) -> Result<(), NoteStoreError> {
    validate_note_content(&next.content)?;
    if next.id != expected.id || next.created_at != expected.created_at {
        return Err(NoteError::Conflict.into());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    current(&tx, expected)?;
    tx.execute(
        "UPDATE notes SET content=?2,updated_at=?3 WHERE id=?1",
        params![
            next.id.to_string(),
            next.content,
            next.updated_at.to_rfc3339()
        ],
    )?;
    tx.commit()?;
    Ok(())
}

fn current(tx: &Transaction<'_>, expected: &Note) -> Result<NoteDetail, NoteStoreError> {
    let detail = read::detail(tx, expected.id)?.ok_or(NoteError::NotFound)?;
    if detail.note != *expected {
        return Err(NoteError::Conflict.into());
    }
    Ok(detail)
}
