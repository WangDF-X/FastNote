//! 小记只持久化正文；校验与摘要规则见 ARCH-03 §3.4/11.4、NOTE-01。

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::NoteId;

/// 小记正文的 Unicode 字符上限，不按 UTF-8 字节计算。
pub const MAX_NOTE_LENGTH: usize = 20_000;

/// 小记实体；标题和预览不存储。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Note {
    /// 应用层生成的 UUIDv7。
    pub id: NoteId,
    /// 原样保留空白、换行的正文。
    pub content: String,
    /// 创建时间（UTC）。
    pub created_at: DateTime<Utc>,
    /// 最近一次真实变更时间（UTC）。
    pub updated_at: DateTime<Utc>,
}

/// 稳定领域错误；不携带正文或用户输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NoteError {
    /// 空白正文。
    #[error("E_NOTE_EMPTY")]
    Empty,
    /// 文件名不符合 Windows 命名规则。
    #[error("E_NOTE_NAME_INVALID")]
    InvalidName,
    /// 同一目录已存在同名文件或文件夹。
    #[error("E_NOTE_NAME_EXISTS")]
    NameExists,
    /// 正文超过 20,000 字符。
    #[error("E_NOTE_TOO_LONG")]
    TooLong,
    /// 小记已不存在。
    #[error("E_NOT_FOUND")]
    NotFound,
    /// 版本或唯一约束冲突。
    #[error("E_CONFLICT")]
    Conflict,
    /// 撤销不属于最新批次或已经过期。
    #[error("E_CONFLICT")]
    UndoExpired,
}

impl Note {
    /// 构造纯小记，不自行生成 ID 或读取时钟。
    pub fn new(id: NoteId, content: String, now: DateTime<Utc>) -> Result<Self, NoteError> {
        validate_note_content(&content)?;
        Ok(Self {
            id,
            content,
            created_at: now,
            updated_at: now,
        })
    }

    /// 校验后生成新的正文版本；重复保存不改变时间。
    pub fn with_content(&self, content: String, now: DateTime<Utc>) -> Result<Self, NoteError> {
        validate_note_content(&content)?;
        let mut next = self.clone();
        if content != self.content {
            next.content = content;
            next.updated_at = now.max(self.updated_at + chrono::Duration::microseconds(1));
        }
        Ok(next)
    }
}

/// 小记正文允许空白；绝不 trim 或截断保存的正文。
pub fn validate_note_content(content: &str) -> Result<(), NoteError> {
    if content.chars().take(MAX_NOTE_LENGTH + 1).count() > MAX_NOTE_LENGTH {
        return Err(NoteError::TooLong);
    }
    Ok(())
}

/// 派生首个非空行的 80 字标题，以及至多两行/120 字的预览。
///
/// 仓储可传入去掉开头空白后的 121 字片段，不需要把整篇正文送给列表。
pub fn note_excerpt(content: &str) -> (String, String) {
    let text = content.trim_start();
    let title = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .chars()
        .take(80)
        .collect();
    let preview = text
        .lines()
        .take(2)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(120)
        .collect();
    (title, preview)
}

#[cfg(test)]
mod tests;
