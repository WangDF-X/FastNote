use fastnote_core::{
    note_excerpt, DateTime, Note, NoteDateGroup, NoteDetail, NoteId, NoteListPage, NoteListQuery,
    NoteSummary, TaskClock, Utc,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::StoreError;

use super::NoteStoreError;

// 与 Rust str::trim_start 的 Unicode White_Space 集合一致；SQL 仅返回 121 字摘要。
const WHITESPACE: &str = " \t\n\r\u{b}\u{c}\u{85}\u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";

pub(super) fn list(
    connection: &Connection,
    query: &NoteListQuery,
    clock: &impl TaskClock,
) -> Result<NoteListPage, NoteStoreError> {
    let tx = connection.unchecked_transaction()?;
    let mut page = NoteListPage::default();
    let mut selected = Vec::new();
    let mut previous = None;
    {
        // 扫描索引中的 ID/时间，不读取正文；分页计数与摘要处于同一读快照。
        let mut statement =
            tx.prepare("SELECT id,updated_at FROM notes ORDER BY updated_at DESC,id DESC")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let at = instant(row.get(1)?)?;
            let date = clock.local_date(at);
            page.total += 1;
            if previous != Some(date) {
                previous = Some(date);
                page.group_total += 1;
                let index = page.group_total - 1;
                if index >= query.group_offset && index < query.group_offset.saturating_add(10) {
                    let offset = query
                        .offsets
                        .iter()
                        .find(|(day, _)| *day == date)
                        .map_or(0, |(_, value)| *value);
                    page.groups.push(NoteDateGroup {
                        date,
                        total: 0,
                        offset,
                        items: Vec::new(),
                    });
                }
            }
            if let Some(group) = page.groups.last_mut().filter(|group| group.date == date) {
                group.total += 1;
                if group.total > group.offset && group.total <= group.offset.saturating_add(20) {
                    selected.push((page.groups.len() - 1, row.get::<_, String>(0)?, at));
                }
            }
        }
    }
    {
        let mut statement =
            tx.prepare("SELECT substr(ltrim(content,?2),1,121),name FROM notes WHERE id=?1")?;
        for (group, id, updated_at) in selected {
            let (fragment, name): (String, String) = statement
                .query_row(params![id, WHITESPACE], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            let (title, preview) = note_excerpt(&fragment);
            page.groups[group].items.push(NoteSummary {
                id: note_id(id)?,
                title: if name.is_empty() { title } else { name },
                preview,
                updated_at,
            });
        }
    }
    tx.commit()?;
    Ok(page)
}

pub(super) fn detail(
    connection: &Connection,
    id: NoteId,
) -> Result<Option<NoteDetail>, NoteStoreError> {
    let raw = connection
        .query_row(
            "SELECT content,created_at,updated_at FROM notes WHERE id=?1",
            [id.to_string()],
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
        Ok(NoteDetail {
            note: Note {
                id,
                content,
                created_at: instant(created)?,
                updated_at: instant(updated)?,
            },
        })
    })
    .transpose()
}

fn instant(value: String) -> Result<DateTime<Utc>, NoteStoreError> {
    value.parse().map_err(|_| StoreError::integrity().into())
}
fn note_id(value: String) -> Result<NoteId, NoteStoreError> {
    value.parse().map_err(|_| StoreError::integrity().into())
}
