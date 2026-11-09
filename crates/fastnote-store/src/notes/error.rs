use fastnote_core::NoteError;
use thiserror::Error;

use crate::StoreError;

/// 小记仓储的稳定领域码、存储源错误与日志 ID。
#[derive(Debug, Error)]
pub enum NoteStoreError {
    /// 输入、版本或关联校验失败。
    #[error("Note error: {0}")]
    Domain(#[from] NoteError),
    /// 约束冲突，保留可追溯的源错误。
    #[error("Note constraint conflict: {source}")]
    Conflict {
        /// 不直接展示或记录源错误字符串。
        #[source]
        source: StoreError,
    },
    /// 数据库或 IO 失败。
    #[error("Note storage error: {0}")]
    Storage(#[from] StoreError),
}

impl NoteStoreError {
    /// 界面只根据稳定错误码映射中文文案。
    pub fn domain_error(&self) -> Option<NoteError> {
        match self {
            Self::Domain(error) => Some(*error),
            Self::Conflict { .. } => Some(NoteError::Conflict),
            Self::Storage(_) => None,
        }
    }
    /// 复用安全诊断流程，不记录 SQL 或正文。
    pub fn storage_error(&self) -> Option<&StoreError> {
        match self {
            Self::Storage(error) | Self::Conflict { source: error } => Some(error),
            Self::Domain(_) => None,
        }
    }
}

impl From<rusqlite::Error> for NoteStoreError {
    fn from(error: rusqlite::Error) -> Self {
        if error.sqlite_error_code() == Some(rusqlite::ffi::ErrorCode::ConstraintViolation) {
            Self::Conflict {
                source: StoreError::from(error),
            }
        } else {
            Self::Storage(StoreError::from(error))
        }
    }
}
