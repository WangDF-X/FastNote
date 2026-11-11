use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    JournalAttachment, JournalAttachmentPayload, JournalEntry, JournalError, JournalImageUsage,
    JournalSummary,
};

/// 日记仓储；按本地日期唯一，空白正文与图片的保留规则见 DIARY-01。
pub trait JournalRepository {
    /// 可追溯的存储错误，保留领域码。
    type Error: From<JournalError>;

    /// 读取单日日记正文；缺失返回 None。
    fn journal_entry(&self, entry_date: NaiveDate) -> Result<Option<JournalEntry>, Self::Error>;

    /// 读取闭区间内的摘要，按日期倒序；不读取全部历史。
    fn journal_summaries(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<JournalSummary>, Self::Error>;

    /// 原子保存或删除单日条目。
    ///
    /// `expected_updated_at` 是保存基线：新建时传 None，已有条目必须与库内版本一致，
    /// 否则返回 [`JournalError::Conflict`]，避免迟到回包覆盖新正文。
    fn save_journal_entry(
        &mut self,
        entry_date: NaiveDate,
        content: &str,
        expected_updated_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Option<JournalEntry>, Self::Error>;

    /// 读取单日附件元数据，按 position 升序。
    fn journal_attachments(
        &self,
        entry_date: NaiveDate,
    ) -> Result<Vec<JournalAttachment>, Self::Error>;

    /// 读取单张附件的原图与缩略图字节。
    fn journal_attachment_payload(
        &self,
        id: &str,
    ) -> Result<Option<JournalAttachmentPayload>, Self::Error>;

    /// 在事务内确保日记行存在、检查单日上限并按顺序追加附件。
    fn add_journal_attachment(
        &mut self,
        attachment: &JournalAttachment,
        bytes: &[u8],
        thumbnail: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(JournalEntry, JournalAttachment), Self::Error>;

    /// 删除附件；正文为空且没有其他图片时同时删除整日条目。
    /// 返回删除后的条目，条目被删除时为 None。
    fn remove_journal_attachment(&mut self, id: &str) -> Result<Option<JournalEntry>, Self::Error>;

    /// 全部日记图片的张数与字节合计。
    fn journal_image_usage(&self) -> Result<JournalImageUsage, Self::Error>;
}
