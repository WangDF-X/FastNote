use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    validate_attachment, validate_journal_content, JournalAttachment, JournalEntry, JournalError,
    JournalRepository, TaskClock,
};

/// 日记写命令，均由唯一 DB 工作线程执行。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalCommand {
    /// 保存单日正文；空白正文在无图片时删除该日条目。
    Save {
        /// 设备本地日期。
        entry_date: NaiveDate,
        /// Markdown 源码。
        content: String,
        /// 保存基线；新建时为 None。
        expected_updated_at: Option<DateTime<Utc>>,
    },
    /// 追加一张已解码并生成缩略图的图片。
    AddAttachment {
        /// 元数据；position 由仓储分配。
        attachment: JournalAttachment,
        /// 原图字节。
        bytes: Vec<u8>,
        /// 缩略图字节。
        thumbnail: Vec<u8>,
    },
    /// 删除一张图片；最后一张且正文为空时同时删除日记条目。
    RemoveAttachment {
        /// 附件 ID。
        id: String,
    },
}

/// 已提交的保存结果；None 表示空白正文删除了条目。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JournalCommandResult {
    /// 保存后的条目版本。
    pub entry: Option<JournalEntry>,
    /// 新增的附件（含仓储分配的位置）。
    pub attachment: Option<JournalAttachment>,
}

/// 注入仓储与时钟，不自行访问 UI 或启动线程。
pub struct JournalService<'a, R: JournalRepository, C: TaskClock> {
    repository: &'a mut R,
    clock: C,
}

impl<'a, R: JournalRepository, C: TaskClock> JournalService<'a, R, C> {
    /// 组合仓储与 UTC 时钟。
    pub fn new(repository: &'a mut R, clock: C) -> Self {
        Self { repository, clock }
    }

    /// 执行一次写操作，由仓储保证事务与版本校验。
    pub fn execute(&mut self, command: JournalCommand) -> Result<JournalCommandResult, R::Error> {
        match command {
            JournalCommand::Save {
                entry_date,
                content,
                expected_updated_at,
            } => {
                validate_journal_content(&content)?;
                let now = self.clock.now();
                let entry = self.repository.save_journal_entry(
                    entry_date,
                    &content,
                    expected_updated_at,
                    now,
                )?;
                Ok(JournalCommandResult {
                    entry,
                    attachment: None,
                })
            }
            JournalCommand::AddAttachment {
                attachment,
                bytes,
                thumbnail,
            } => {
                validate_attachment(&attachment, &bytes, &thumbnail).map_err(JournalError::from)?;
                let now = self.clock.now();
                let (entry, attachment) =
                    self.repository
                        .add_journal_attachment(&attachment, &bytes, &thumbnail, now)?;
                Ok(JournalCommandResult {
                    entry: Some(entry),
                    attachment: Some(attachment),
                })
            }
            JournalCommand::RemoveAttachment { id } => {
                let entry = self.repository.remove_journal_attachment(&id)?;
                Ok(JournalCommandResult {
                    entry,
                    attachment: None,
                })
            }
        }
    }
}
