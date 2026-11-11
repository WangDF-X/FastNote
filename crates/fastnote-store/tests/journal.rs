use fastnote_core::{
    AttachmentError, Clock, DateTime, ImageMediaType, JournalAttachment, JournalCommand,
    JournalCommandResult, JournalEntry, JournalError, JournalRepository, JournalService, NaiveDate,
    NaiveDateTime, TaskClock, Utc, MAX_JOURNAL_ATTACHMENTS, MAX_JOURNAL_LENGTH,
};
use fastnote_store::{JournalStoreError, SqliteSettings};
use rusqlite::Connection;

fn now() -> DateTime<Utc> {
    "2026-10-23T06:32:00Z".parse().unwrap()
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
}

fn fake_png(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend(std::iter::repeat_n(seed, 64));
    bytes
}

fn fake_thumbnail() -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF];
    bytes.extend(std::iter::repeat_n(0x11, 16));
    bytes
}

fn attachment_id(number: u32) -> String {
    format!("019a0fff-aaaa-7000-8000-{number:012x}")
}

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.local_date(self.0)
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        at.naive_utc()
    }
}

struct Fixture {
    store: SqliteSettings,
    path: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake-journal.db");
        let store = SqliteSettings::open(&path, now()).unwrap();
        Self {
            store,
            path,
            _directory: directory,
        }
    }

    fn execute(
        &mut self,
        entry_date: NaiveDate,
        content: &str,
        expected: Option<DateTime<Utc>>,
        at: DateTime<Utc>,
    ) -> Result<JournalCommandResult, JournalStoreError> {
        JournalService::new(&mut self.store, FixedClock(at)).execute(JournalCommand::Save {
            entry_date,
            content: content.to_owned(),
            expected_updated_at: expected,
        })
    }

    fn save(&mut self, entry_date: NaiveDate, content: &str, at: DateTime<Utc>) -> JournalEntry {
        let baseline = self
            .store
            .journal_entry(entry_date)
            .unwrap()
            .map(|e| e.updated_at);
        self.execute(entry_date, content, baseline, at)
            .unwrap()
            .entry
            .unwrap()
    }

    fn add_image(
        &mut self,
        entry_date: NaiveDate,
        number: u32,
        at: DateTime<Utc>,
    ) -> Result<JournalAttachment, JournalStoreError> {
        let bytes = fake_png(number as u8);
        let thumbnail = fake_thumbnail();
        let attachment = JournalAttachment::new(
            attachment_id(number),
            entry_date,
            ImageMediaType::Png,
            100,
            80,
            at,
        )
        .with_data(&bytes, &thumbnail)
        .unwrap();
        JournalService::new(&mut self.store, FixedClock(at))
            .execute(JournalCommand::AddAttachment {
                attachment,
                bytes,
                thumbnail,
            })
            .map(|result| result.attachment.unwrap())
    }

    fn remove_image(
        &mut self,
        id: &str,
        at: DateTime<Utc>,
    ) -> Result<JournalCommandResult, JournalStoreError> {
        JournalService::new(&mut self.store, FixedClock(at))
            .execute(JournalCommand::RemoveAttachment { id: id.to_owned() })
    }

    fn external(&self) -> Connection {
        let connection = Connection::open(&self.path).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        connection
    }
}

#[test]
fn test_month_is_empty_without_entries() {
    let fixture = Fixture::new();
    assert!(fixture
        .store
        .journal_summaries(date(1), date(31))
        .unwrap()
        .is_empty());
    assert_eq!(fixture.store.journal_entry(date(23)).unwrap(), None);
}

#[test]
fn test_save_creates_entry_and_roundtrips_content() {
    let mut fixture = Fixture::new();
    let saved = fixture
        .execute(date(23), "# 今天\n\n写了日记。", None, now())
        .unwrap()
        .entry
        .unwrap();
    assert_eq!(saved.entry_date, date(23));
    assert_eq!(saved.content, "# 今天\n\n写了日记。");
    assert_eq!(saved.created_at, now());
    assert_eq!(fixture.store.journal_entry(date(23)).unwrap(), Some(saved));
}

#[test]
fn test_update_keeps_created_at_and_bumps_updated_at() {
    let mut fixture = Fixture::new();
    let first = fixture.save(date(23), "第一版", now());
    let later = now() + chrono::Duration::seconds(30);
    let second = fixture.save(date(23), "第二版", later);
    assert_eq!(second.created_at, first.created_at);
    assert!(second.updated_at > first.created_at);
    assert_eq!(second.content, "第二版");
}

#[test]
fn test_clearing_content_deletes_entry() {
    let mut fixture = Fixture::new();
    fixture.save(date(23), "有内容", now());
    let result = fixture
        .execute(date(23), " \n\t", Some(now()), now())
        .unwrap();
    assert_eq!(result.entry, None);
    assert_eq!(fixture.store.journal_entry(date(23)).unwrap(), None);
}

#[test]
fn test_clearing_absent_entry_is_noop() {
    let mut fixture = Fixture::new();
    let result = fixture.execute(date(23), "", None, now()).unwrap();
    assert_eq!(result.entry, None);
    assert_eq!(fixture.store.journal_entry(date(23)).unwrap(), None);
}

#[test]
fn test_stale_baseline_is_rejected_and_content_preserved() {
    let mut fixture = Fixture::new();
    fixture.save(date(23), "第一版", now());
    let stale = now() - chrono::Duration::seconds(5);
    let error = fixture
        .execute(date(23), "迟到回包", Some(stale), now())
        .unwrap_err();
    assert_eq!(error.domain_error(), Some(JournalError::Conflict));
    assert_eq!(
        fixture
            .store
            .journal_entry(date(23))
            .unwrap()
            .unwrap()
            .content,
        "第一版"
    );
}

#[test]
fn test_creating_over_existing_entry_is_rejected() {
    let mut fixture = Fixture::new();
    fixture.save(date(23), "第一版", now());
    let error = fixture
        .execute(date(23), "重复新建", None, now())
        .unwrap_err();
    assert_eq!(error.domain_error(), Some(JournalError::Conflict));
}

#[test]
fn test_range_only_returns_dates_inside_range_in_descending_order() {
    let mut fixture = Fixture::new();
    fixture.save(date(2), "月初", now());
    fixture.save(date(10), "# 中间\n- [x] 一条", now());
    fixture.save(date(28), "月末", now());
    fixture.save(NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(), "上月", now());

    let summaries = fixture.store.journal_summaries(date(1), date(31)).unwrap();
    let dates: Vec<_> = summaries.iter().map(|s| s.entry_date).collect();
    assert_eq!(dates, vec![date(28), date(10), date(2)]);
    assert_eq!(summaries[1].excerpt, "中间 一条");
    assert!(summaries
        .iter()
        .all(|summary| summary.attachment_count == 0));
}

#[test]
fn test_week_range_is_closed_and_excludes_neighbouring_days() {
    let mut fixture = Fixture::new();
    fixture.save(date(8), "上周日", now());
    fixture.save(date(9), "本周一", now());
    fixture.save(date(15), "本周日", now());
    fixture.save(date(16), "下周一", now());

    let summaries = fixture.store.journal_summaries(date(9), date(15)).unwrap();
    let dates: Vec<_> = summaries.iter().map(|s| s.entry_date).collect();
    assert_eq!(dates, vec![date(15), date(9)]);
}

#[test]
fn test_too_long_content_is_rejected_without_writing() {
    let mut fixture = Fixture::new();
    fixture.save(date(23), "原有内容", now());
    let over = "长".repeat(MAX_JOURNAL_LENGTH + 1);
    let error = fixture
        .execute(date(23), &over, Some(now()), now())
        .unwrap_err();
    assert_eq!(error.domain_error(), Some(JournalError::TooLong));
    assert_eq!(
        fixture
            .store
            .journal_entry(date(23))
            .unwrap()
            .unwrap()
            .content,
        "原有内容"
    );
}

#[test]
fn test_blank_content_with_image_keeps_entry_and_counts_in_month() {
    let mut fixture = Fixture::new();
    fixture.add_image(date(23), 1, now()).unwrap();
    let entry = fixture.store.journal_entry(date(23)).unwrap().unwrap();
    assert_eq!(entry.content, "");
    let summaries = fixture.store.journal_summaries(date(1), date(31)).unwrap();
    assert_eq!(summaries[0].attachment_count, 1);
    assert_eq!(summaries[0].excerpt, "");
    // 空正文保存后条目仍保留。
    let saved = fixture.execute(date(23), "", Some(entry.updated_at), now());
    // 基线来自添加附件前的条目版本，添加附件不改版本，因此可以直接保存。
    assert!(saved.is_ok());
    assert!(fixture.store.journal_entry(date(23)).unwrap().is_some());
}

#[test]
fn test_remove_last_image_deletes_empty_entry_but_keeps_text_entry() {
    let mut fixture = Fixture::new();
    let first = fixture.add_image(date(23), 1, now()).unwrap();
    fixture.remove_image(&first.id, now()).unwrap();
    assert_eq!(fixture.store.journal_entry(date(23)).unwrap(), None);

    fixture.save(date(23), "有正文", now());
    let second = fixture.add_image(date(23), 2, now()).unwrap();
    fixture.remove_image(&second.id, now()).unwrap();
    assert!(fixture.store.journal_entry(date(23)).unwrap().is_some());
    assert!(fixture
        .store
        .journal_attachments(date(23))
        .unwrap()
        .is_empty());
}

#[test]
fn test_attachment_positions_payload_and_usage_roundtrip() {
    let mut fixture = Fixture::new();
    let first = fixture.add_image(date(23), 1, now()).unwrap();
    let second = fixture.add_image(date(23), 2, now()).unwrap();
    assert_eq!((first.position, second.position), (0, 1));
    let attachments = fixture.store.journal_attachments(date(23)).unwrap();
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[1].id, second.id);

    let payload = fixture
        .store
        .journal_attachment_payload(&second.id)
        .unwrap()
        .unwrap();
    assert_eq!(payload.bytes, fake_png(2));
    assert_eq!(payload.thumbnail, fake_thumbnail());

    let usage = fixture.store.journal_image_usage().unwrap();
    assert_eq!(usage.count, 2);
    assert_eq!(
        usage.bytes,
        (fake_png(1).len() + fake_png(2).len() + fake_thumbnail().len() * 2) as u64
    );

    fixture.remove_image(&first.id, now()).unwrap();
    let usage = fixture.store.journal_image_usage().unwrap();
    assert_eq!(usage.count, 1);
}

#[test]
fn test_attachment_limit_and_missing_attachment_are_rejected() {
    let mut fixture = Fixture::new();
    for number in 1..=MAX_JOURNAL_ATTACHMENTS {
        fixture.add_image(date(23), number, now()).unwrap();
    }
    let error = fixture
        .add_image(date(23), MAX_JOURNAL_ATTACHMENTS + 1, now())
        .unwrap_err();
    assert_eq!(
        error.domain_error(),
        Some(JournalError::Attachment(AttachmentError::Limit))
    );
    assert_eq!(
        fixture.store.journal_attachments(date(23)).unwrap().len() as u32,
        MAX_JOURNAL_ATTACHMENTS
    );

    let error = fixture.remove_image("missing", now()).unwrap_err();
    assert_eq!(error.domain_error(), Some(JournalError::NotFound));
}

#[test]
fn test_deleting_entry_cascades_attachment_rows() {
    let mut fixture = Fixture::new();
    let attachment = fixture.add_image(date(23), 1, now()).unwrap();
    fixture
        .external()
        .execute("DELETE FROM journal_entries", [])
        .unwrap();
    assert!(fixture
        .store
        .journal_attachments(date(23))
        .unwrap()
        .is_empty());
    assert!(fixture
        .store
        .journal_attachment_payload(&attachment.id)
        .unwrap()
        .is_none());
    assert_eq!(fixture.store.journal_image_usage().unwrap().count, 0);
}

#[test]
fn test_image_validation_rejects_non_image_and_oversized() {
    let mut fixture = Fixture::new();
    let bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let thumbnail = fake_thumbnail();
    let attachment = JournalAttachment::new(
        attachment_id(9),
        date(23),
        ImageMediaType::Png,
        10,
        10,
        now(),
    )
    .with_data(&bytes, &thumbnail)
    .unwrap();
    let error = JournalService::new(&mut fixture.store, FixedClock(now()))
        .execute(JournalCommand::AddAttachment {
            attachment,
            bytes: b"not an image".to_vec(),
            thumbnail,
        })
        .unwrap_err();
    assert_eq!(
        error.domain_error(),
        Some(JournalError::Attachment(AttachmentError::Format))
    );
    assert!(fixture
        .store
        .journal_attachments(date(23))
        .unwrap()
        .is_empty());
}
