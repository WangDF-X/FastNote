use std::path::Path;

use fastnote_core::{
    JournalAttachment, JournalCommand, JournalCommandResult, JournalEntry, JournalError,
    JournalImageUsage, JournalRepository, JournalService, JournalSummary, NaiveDate, TaskClock,
};
use fastnote_store::{JournalStoreError, SqliteSettings, StoreError};
use uuid::Uuid;

use crate::clock::SystemClock;
use crate::logging::storage_error;

use super::image::{self, ImageError};

pub(crate) enum JournalRead {
    /// 单日正文；缺失返回 None。
    Entry(NaiveDate),
    /// 闭区间内的摘要，按日期倒序；月视图和周视图共用。
    Summaries { start: NaiveDate, end: NaiveDate },
    /// 单日附件缩略图与全局图片占用。
    Attachments(NaiveDate),
    /// 单张大图，供查看器使用。
    AttachmentView(String),
}

/// 已解码的缩略图或查看器图片；UI 线程只负责转成 Slint 图像。
pub(crate) struct AttachmentView {
    pub(crate) attachment: JournalAttachment,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

pub(crate) enum JournalReadResult {
    Entry {
        date: NaiveDate,
        entry: Option<Box<JournalEntry>>,
    },
    Summaries(Vec<JournalSummary>),
    Attachments {
        date: NaiveDate,
        views: Vec<AttachmentView>,
        usage: JournalImageUsage,
    },
    AttachmentView(AttachmentView),
}

pub(crate) fn execute(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    command: JournalCommand,
) -> Result<JournalCommandResult, JournalStoreError> {
    let result = store
        .ok_or_else(unavailable)
        .and_then(|store| JournalService::new(store, SystemClock).execute(command));
    diagnose(path, "journal_command", &result);
    result
}

/// 在 DB 工作线程读取文件、解码、生成缩略图，再走同一写命令。
pub(crate) fn add_image(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    entry_date: NaiveDate,
    source: &Path,
) -> Result<JournalCommandResult, JournalStoreError> {
    let prepared = image::prepare(source).map_err(image_error)?;
    let now = SystemClock.now();
    let attachment = JournalAttachment::new(
        Uuid::now_v7().to_string(),
        entry_date,
        prepared.media_type,
        prepared.width,
        prepared.height,
        now,
    )
    .with_data(&prepared.bytes, &prepared.thumbnail)
    .map_err(|error| JournalStoreError::Domain(JournalError::Attachment(error)))?;
    execute(
        store,
        path,
        JournalCommand::AddAttachment {
            attachment,
            bytes: prepared.bytes,
            thumbnail: prepared.thumbnail,
        },
    )
}

pub(crate) fn query(
    store: Option<&mut SqliteSettings>,
    path: Option<&Path>,
    query: JournalRead,
) -> Result<JournalReadResult, JournalStoreError> {
    let result = store.ok_or_else(unavailable).and_then(|store| match query {
        JournalRead::Entry(date) => {
            store
                .journal_entry(date)
                .map(|entry| JournalReadResult::Entry {
                    date,
                    entry: entry.map(Box::new),
                })
        }
        JournalRead::Summaries { start, end } => store
            .journal_summaries(start, end)
            .map(JournalReadResult::Summaries),
        JournalRead::Attachments(date) => {
            let attachments = store.journal_attachments(date)?;
            let usage = store.journal_image_usage()?;
            let mut views = Vec::with_capacity(attachments.len());
            for attachment in attachments {
                let payload = store
                    .journal_attachment_payload(&attachment.id)?
                    .ok_or(JournalError::NotFound)?;
                let decoded = image::decode_thumbnail(&payload.thumbnail).map_err(image_error)?;
                views.push(AttachmentView {
                    attachment,
                    width: decoded.width,
                    height: decoded.height,
                    rgba: decoded.rgba,
                });
            }
            Ok(JournalReadResult::Attachments { date, views, usage })
        }
        JournalRead::AttachmentView(id) => {
            let payload = store
                .journal_attachment_payload(&id)?
                .ok_or(JournalError::NotFound)?;
            let decoded = image::decode_view(&payload.bytes).map_err(image_error)?;
            Ok(JournalReadResult::AttachmentView(AttachmentView {
                attachment: payload.attachment,
                width: decoded.width,
                height: decoded.height,
                rgba: decoded.rgba,
            }))
        }
    });
    diagnose(path, "journal_query", &result);
    result
}

fn image_error(error: ImageError) -> JournalStoreError {
    JournalStoreError::Domain(JournalError::Attachment(error.attachment_error()))
}

fn unavailable() -> JournalStoreError {
    StoreError::from(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "journal store unavailable",
    ))
    .into()
}

fn diagnose<T>(
    path: Option<&Path>,
    operation: &'static str,
    result: &Result<T, JournalStoreError>,
) {
    if let Err(error) = result {
        if let Some(error) = error.storage_error() {
            storage_error(path, operation, error);
        } else if let Some(error) = error.domain_error() {
            tracing::warn!(operation, error_code = ?error, "journal operation rejected");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{Rgb, RgbImage};
    use fastnote_core::{JournalCommand, MAX_JOURNAL_ATTACHMENT_BYTES};

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn write_png(path: &Path) {
        RgbImage::from_pixel(64, 32, Rgb([20, 40, 60]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn test_add_image_decodes_stores_lists_and_removes_last_entry() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("fake.db");
        let mut store = SqliteSettings::open(&database, chrono::Utc::now()).unwrap();
        let source = directory.path().join("fake.png");
        write_png(&source);

        let result = add_image(Some(&mut store), None, date(), &source).unwrap();
        let attachment = result.attachment.unwrap();
        assert_eq!((attachment.width, attachment.height), (64, 32));
        assert_eq!(attachment.position, 0);
        assert!(attachment.thumbnail_size > 0);

        let reply = query(Some(&mut store), None, JournalRead::Attachments(date())).unwrap();
        match reply {
            JournalReadResult::Attachments { views, usage, .. } => {
                assert_eq!(views.len(), 1);
                assert_eq!(usage.count, 1);
                assert_eq!(
                    views[0].rgba.len(),
                    (views[0].width * views[0].height * 4) as usize
                );
            }
            _ => panic!("expected attachments"),
        }

        let view = query(
            Some(&mut store),
            None,
            JournalRead::AttachmentView(attachment.id.clone()),
        )
        .unwrap();
        assert!(matches!(view, JournalReadResult::AttachmentView(_)));

        execute(
            Some(&mut store),
            None,
            JournalCommand::RemoveAttachment { id: attachment.id },
        )
        .unwrap();
        assert_eq!(store.journal_entry(date()).unwrap(), None);
    }

    #[test]
    fn test_add_image_rejects_oversized_and_non_image_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("fake.db");
        let mut store = SqliteSettings::open(&database, chrono::Utc::now()).unwrap();
        let text = directory.path().join("fake.png");
        std::fs::write(&text, b"not an image").unwrap();
        let error = add_image(Some(&mut store), None, date(), &text).unwrap_err();
        assert_eq!(
            error.domain_error(),
            Some(JournalError::Attachment(
                fastnote_core::AttachmentError::Format
            ))
        );
        assert_eq!(store.journal_entry(date()).unwrap(), None);

        let large = directory.path().join("large.png");
        let file = std::fs::File::create(&large).unwrap();
        file.set_len(MAX_JOURNAL_ATTACHMENT_BYTES as u64 + 1)
            .unwrap();
        let error = add_image(Some(&mut store), None, date(), &large).unwrap_err();
        assert_eq!(
            error.domain_error(),
            Some(JournalError::Attachment(
                fastnote_core::AttachmentError::TooLarge
            ))
        );
        assert_eq!(store.journal_entry(date()).unwrap(), None);
    }
}
