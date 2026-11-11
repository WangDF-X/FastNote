mod error;
mod read;
mod write;

use chrono::{DateTime, NaiveDate, Utc};
use fastnote_core::{
    JournalAttachment, JournalAttachmentPayload, JournalEntry, JournalImageUsage,
    JournalRepository, JournalSummary,
};

use crate::SqliteSettings;

pub use error::JournalStoreError;

impl JournalRepository for SqliteSettings {
    type Error = JournalStoreError;

    fn journal_entry(&self, entry_date: NaiveDate) -> Result<Option<JournalEntry>, Self::Error> {
        read::entry(&self.connection, entry_date)
    }

    fn journal_summaries(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<JournalSummary>, Self::Error> {
        read::summaries(&self.connection, start, end)
    }

    fn save_journal_entry(
        &mut self,
        entry_date: NaiveDate,
        content: &str,
        expected_updated_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Option<JournalEntry>, Self::Error> {
        write::save(
            &mut self.connection,
            entry_date,
            content,
            expected_updated_at,
            now,
        )
    }

    fn journal_attachments(
        &self,
        entry_date: NaiveDate,
    ) -> Result<Vec<JournalAttachment>, Self::Error> {
        read::attachments(&self.connection, entry_date)
    }

    fn journal_attachment_payload(
        &self,
        id: &str,
    ) -> Result<Option<JournalAttachmentPayload>, Self::Error> {
        read::attachment_payload(&self.connection, id)
    }

    fn add_journal_attachment(
        &mut self,
        attachment: &JournalAttachment,
        bytes: &[u8],
        thumbnail: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(JournalEntry, JournalAttachment), Self::Error> {
        write::add_attachment(&mut self.connection, attachment, bytes, thumbnail, now)
    }

    fn remove_journal_attachment(&mut self, id: &str) -> Result<Option<JournalEntry>, Self::Error> {
        write::remove_attachment(&mut self.connection, id)
    }

    fn journal_image_usage(&self) -> Result<JournalImageUsage, Self::Error> {
        read::image_usage(&self.connection)
    }
}
