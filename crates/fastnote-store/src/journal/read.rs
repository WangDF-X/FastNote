use chrono::{DateTime, NaiveDate, Utc};
use fastnote_core::{
    journal_excerpt, ImageMediaType, JournalAttachment, JournalAttachmentPayload, JournalEntry,
    JournalImageUsage, JournalSummary,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::StoreError;

use super::JournalStoreError;

pub(super) fn entry(
    connection: &Connection,
    entry_date: NaiveDate,
) -> Result<Option<JournalEntry>, JournalStoreError> {
    let raw = connection
        .query_row(
            "SELECT content,created_at,updated_at FROM journal_entries WHERE entry_date=?1",
            [entry_date.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    raw.map(|(content, created, updated)| {
        Ok(JournalEntry {
            entry_date,
            content,
            created_at: instant(created)?,
            updated_at: instant(updated)?,
        })
    })
    .transpose()
}

/// 只读取给定闭区间内的条目，按日期倒序返回摘要。
pub(super) fn summaries(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<JournalSummary>, JournalStoreError> {
    let mut statement = connection.prepare(
        "SELECT e.entry_date,e.content,e.updated_at,
            (SELECT COUNT(*) FROM journal_attachments a WHERE a.entry_date=e.entry_date)
         FROM journal_entries e
         WHERE e.entry_date BETWEEN ?1 AND ?2 ORDER BY e.entry_date DESC",
    )?;
    let rows = statement.query_map(params![start.to_string(), end.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut summaries = Vec::new();
    for row in rows {
        let (date, content, updated, attachments) = row?;
        summaries.push(JournalSummary {
            entry_date: date_value(date)?,
            excerpt: journal_excerpt(&content),
            updated_at: instant(updated)?,
            attachment_count: u32::try_from(attachments).map_err(|_| StoreError::integrity())?,
        });
    }
    Ok(summaries)
}

pub(super) fn attachments(
    connection: &Connection,
    entry_date: NaiveDate,
) -> Result<Vec<JournalAttachment>, JournalStoreError> {
    let mut statement = connection.prepare(
        "SELECT id,position,media_type,width,height,byte_size,thumbnail_size,created_at
         FROM journal_attachments WHERE entry_date=?1 ORDER BY position,id",
    )?;
    let rows = statement.query_map([entry_date.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let mut attachments = Vec::new();
    for row in rows {
        let (id, position, media_type, width, height, byte_size, thumbnail_size, created) = row?;
        attachments.push(JournalAttachment {
            id,
            entry_date,
            position: u32::try_from(position).map_err(|_| StoreError::integrity())?,
            media_type: ImageMediaType::parse(&media_type).ok_or_else(StoreError::integrity)?,
            width: u32::try_from(width).map_err(|_| StoreError::integrity())?,
            height: u32::try_from(height).map_err(|_| StoreError::integrity())?,
            byte_size: u32::try_from(byte_size).map_err(|_| StoreError::integrity())?,
            thumbnail_size: u32::try_from(thumbnail_size).map_err(|_| StoreError::integrity())?,
            created_at: instant(created)?,
        });
    }
    Ok(attachments)
}

pub(super) fn attachment_payload(
    connection: &Connection,
    id: &str,
) -> Result<Option<JournalAttachmentPayload>, JournalStoreError> {
    let raw = connection
        .query_row(
            "SELECT a.entry_date,a.position,a.media_type,a.width,a.height,a.byte_size,
                a.thumbnail_size,a.created_at,d.bytes,d.thumbnail
             FROM journal_attachments a
             JOIN journal_attachment_data d ON d.attachment_id=a.id
             WHERE a.id=?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                    row.get::<_, Vec<u8>>(9)?,
                ))
            },
        )
        .optional()?;
    raw.map(
        |(
            entry_date,
            position,
            media_type,
            width,
            height,
            byte_size,
            thumbnail_size,
            created,
            bytes,
            thumbnail,
        )| {
            Ok(JournalAttachmentPayload {
                attachment: JournalAttachment {
                    id: id.to_owned(),
                    entry_date: date_value(entry_date)?,
                    position: u32::try_from(position).map_err(|_| StoreError::integrity())?,
                    media_type: ImageMediaType::parse(&media_type)
                        .ok_or_else(StoreError::integrity)?,
                    width: u32::try_from(width).map_err(|_| StoreError::integrity())?,
                    height: u32::try_from(height).map_err(|_| StoreError::integrity())?,
                    byte_size: u32::try_from(byte_size).map_err(|_| StoreError::integrity())?,
                    thumbnail_size: u32::try_from(thumbnail_size)
                        .map_err(|_| StoreError::integrity())?,
                    created_at: instant(created)?,
                },
                bytes,
                thumbnail,
            })
        },
    )
    .transpose()
}

pub(super) fn image_usage(connection: &Connection) -> Result<JournalImageUsage, JournalStoreError> {
    let (count, bytes): (i64, i64) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(byte_size + thumbnail_size), 0) FROM journal_attachments",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(JournalImageUsage {
        count: u32::try_from(count).map_err(|_| StoreError::integrity())?,
        bytes: u64::try_from(bytes).map_err(|_| StoreError::integrity())?,
    })
}

fn date_value(value: String) -> Result<NaiveDate, JournalStoreError> {
    NaiveDate::parse_from_str(&value, "%Y-%m-%d").map_err(|_| StoreError::integrity().into())
}

fn instant(value: String) -> Result<DateTime<Utc>, JournalStoreError> {
    value.parse().map_err(|_| StoreError::integrity().into())
}
