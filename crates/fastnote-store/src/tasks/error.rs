use fastnote_core::TaskError;
use thiserror::Error;

use crate::StoreError;

/// 任务仓储保留领域码、SQLite 源错误和日志 ID，不外露正文。
#[derive(Debug, Error)]
pub enum TaskStoreError {
    /// 输入、不变量、只读或版本校验失败。
    #[error("Task error: {0}")]
    Domain(#[from] TaskError),
    /// SQLite 约束冲突；保留原始错误链以供安全诊断。
    #[error("Task constraint conflict: {source}")]
    Conflict {
        /// 只可按稳定数字码和日志 ID 记录的存储错误。
        #[source]
        source: StoreError,
    },
    /// IO/数据库错误。
    #[error("Task storage error: {0}")]
    Storage(#[from] StoreError),
}

impl TaskStoreError {
    /// UI 映射使用的稳定领域错误；不读取 Display/source。
    pub fn domain_error(&self) -> Option<TaskError> {
        match self {
            Self::Domain(error) => Some(*error),
            Self::Conflict { .. } => Some(TaskError::Conflict),
            Self::Storage(_) => None,
        }
    }

    /// 存储诊断使用原有安全日志流程，不打印 SQL 或绑定参数。
    pub fn storage_error(&self) -> Option<&StoreError> {
        match self {
            Self::Storage(error) | Self::Conflict { source: error } => Some(error),
            Self::Domain(_) => None,
        }
    }
}

impl From<rusqlite::Error> for TaskStoreError {
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
