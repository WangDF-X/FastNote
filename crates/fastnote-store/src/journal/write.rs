use chrono::{DateTime, NaiveDate, Utc};
use fastnote_core::{
    validate_journal_content, AttachmentError, JournalAttachment, JournalEntry, JournalError,
    MAX_JOURNAL_ATTACHMENTS,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::database::check_write_version;

use super::{read, JournalStoreError};

/// 原子保存或删除单日条目；空白正文在无图片时删除整行（DIARY-01 §5/§5.1）。
pub(super) fn save(
    connection: &mut Connection,
    entry_date: NaiveDate,
    content: &str,
    expected_updated_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<Option<JournalEntry>, JournalStoreError> {
    validate_journal_content(content)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let current = read::entry(&tx, entry_date)?;
    // 新建要求库内无条目；更新要求基线版本与库内一致。
    match (&current, expected_updated_at) {
        (Some(entry), Some(expected)) if entry.updated_at == expected => {}
        (None, None) => {}
        _ => return Err(JournalError::Conflict.into()),
    }
    if content.trim().is_empty() {
        let attachments: i64 = tx.query_row(
            "SELECT COUNT(*) FROM journal_attachments WHERE entry_date=?1",
            [entry_date.to_string()],
            |row| row.get(0),
        )?;
        if current.is_none() {
            tx.commit()?;
            return Ok(None);
        }
        if attachments == 0 {
            tx.execute(
                "DELETE FROM journal_entries WHERE entry_date=?1",
                [entry_date.to_string()],
            )?;
            tx.commit()?;
            return Ok(None);
        }
    }
    let entry = match current {
        Some(existing) => {
            let next = existing.with_content(content.to_owned(), now)?;
            if next.content != existing.content {
                let changed = tx.execute(
                    "UPDATE journal_entries SET content=?1,updated_at=?2
                     WHERE entry_date=?3 AND updated_at=?4",
                    params![
                        next.content,
                        next.updated_at.to_rfc3339(),
                        entry_date.to_string(),
                        existing.updated_at.to_rfc3339()
                    ],
                )?;
                if changed == 0 {
                    return Err(JournalError::Conflict.into());
                }
            }
            next
        }
        None => {
            let entry = JournalEntry::new(entry_date, content.to_owned(), now)?;
            // id 仅作为存储主键；业务唯一性由 entry_date 保证，不暴露给界面。
            tx.execute(
                "INSERT INTO journal_entries(id,entry_date,content,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?4)",
                params![
                    Uuid::now_v7().to_string(),
                    entry_date.to_string(),
                    entry.content,
                    entry.created_at.to_rfc3339()
                ],
            )?;
            entry
        }
    };
    tx.commit()?;
    Ok(Some(entry))
}

/// 确保日记行存在、检查单日上限并追加附件；位置由仓储分配。
pub(super) fn add_attachment(
    connection: &mut Connection,
    attachment: &JournalAttachment,
    bytes: &[u8],
    thumbnail: &[u8],
    now: DateTime<Utc>,
) -> Result<(JournalEntry, JournalAttachment), JournalStoreError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let entry_date = attachment.entry_date;
    let entry = match read::entry(&tx, entry_date)? {
        Some(entry) => entry,
        None => {
            // 正文为空但有图片的日记：content 存空串。
            tx.execute(
                "INSERT INTO journal_entries(id,entry_date,content,created_at,updated_at)
                 VALUES(?1,?2,'',?3,?3)",
                params![
                    Uuid::now_v7().to_string(),
                    entry_date.to_string(),
                    now.to_rfc3339()
                ],
            )?;
            JournalEntry {
                entry_date,
                content: String::new(),
                created_at: now,
                updated_at: now,
            }
        }
    };
    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM journal_attachments WHERE entry_date=?1",
        [entry_date.to_string()],
        |row| row.get(0),
    )?;
    if count >= i64::from(MAX_JOURNAL_ATTACHMENTS) {
        return Err(JournalError::from(AttachmentError::Limit).into());
    }
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM journal_attachments WHERE entry_date=?1",
        [entry_date.to_string()],
        |row| row.get(0),
    )?;
    let position = u32::try_from(position).map_err(|_| crate::StoreError::integrity())?;
    let stored = JournalAttachment {
        position,
        ..attachment.clone()
    };
    tx.execute(
        "INSERT INTO journal_attachments(
            id,entry_date,position,media_type,width,height,byte_size,thumbnail_size,created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            stored.id,
            entry_date.to_string(),
            i64::from(stored.position),
            stored.media_type.as_str(),
            i64::from(stored.width),
            i64::from(stored.height),
            i64::from(stored.byte_size),
            i64::from(stored.thumbnail_size),
            stored.created_at.to_rfc3339()
        ],
    )?;
    tx.execute(
        "INSERT INTO journal_attachment_data(attachment_id,bytes,thumbnail) VALUES(?1,?2,?3)",
        params![stored.id, bytes, thumbnail],
    )?;
    tx.commit()?;
    Ok((entry, stored))
}

/// 删除附件；正文为空且没有其他图片时删除整日条目。
pub(super) fn remove_attachment(
    connection: &mut Connection,
    id: &str,
) -> Result<Option<JournalEntry>, JournalStoreError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let entry_date: Option<String> = tx
        .query_row(
            "SELECT entry_date FROM journal_attachments WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(entry_date) = entry_date else {
        return Err(JournalError::NotFound.into());
    };
    let date = NaiveDate::parse_from_str(&entry_date, "%Y-%m-%d")
        .map_err(|_| crate::StoreError::integrity())?;
    tx.execute(
        "DELETE FROM journal_attachment_data WHERE attachment_id=?1",
        [id],
    )?;
    tx.execute("DELETE FROM journal_attachments WHERE id=?1", [id])?;
    let remaining: i64 = tx.query_row(
        "SELECT COUNT(*) FROM journal_attachments WHERE entry_date=?1",
        [entry_date.clone()],
        |row| row.get(0),
    )?;
    if remaining == 0 {
        let content: String = tx.query_row(
            "SELECT content FROM journal_entries WHERE entry_date=?1",
            [entry_date.clone()],
            |row| row.get(0),
        )?;
        if content.trim().is_empty() {
            tx.execute(
                "DELETE FROM journal_entries WHERE entry_date=?1",
                [entry_date],
            )?;
        }
    }
    let entry = read::entry(&tx, date)?;
    tx.commit()?;
    Ok(entry)
}
