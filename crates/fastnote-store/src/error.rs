use std::io;

use fastnote_core::StorageErrorCode;
use rusqlite::ffi::ErrorCode;
use thiserror::Error;
use uuid::Uuid;

/// 存储错误保留源错误与日志 ID；不可把 Display/source 直接交给 UI。
#[derive(Debug, Error)]
pub enum StoreError {
    /// SQLite 读写失败。
    #[error("SQLite error ({code:?}, log_id={log_id})")]
    Sqlite {
        /// 稳定错误分类。
        code: StorageErrorCode,
        /// 本次错误的关联 ID。
        log_id: String,
        /// 原始 SQLite 错误，仅用于诊断。
        #[source]
        source: rusqlite::Error,
    },
    /// 文件/目录操作失败。
    #[error("File error ({code:?}, log_id={log_id})")]
    File {
        /// 稳定错误分类。
        code: StorageErrorCode,
        /// 本次错误的关联 ID。
        log_id: String,
        /// 原始 IO 错误。
        #[source]
        source: io::Error,
    },
    /// 更高版本的库不得写入。
    #[error("Unsupported schema {found}, supported={supported} (log_id={log_id})")]
    NewerSchema {
        /// 文件中的版本。
        found: i64,
        /// 本应用支持的版本。
        supported: i64,
        /// 本次错误的关联 ID。
        log_id: String,
    },
    /// 非 FastNote 库、版本审计不一致或完整性异常。
    #[error("Database integrity error (log_id={log_id})")]
    Integrity {
        /// 本次错误的关联 ID。
        log_id: String,
    },
    /// 应用层传入了无效/重复的配置值（不记录原值）。
    #[error("Invalid setting key={key} (log_id={log_id})")]
    InvalidSetting {
        /// 已登记的配置键。
        key: &'static str,
        /// 本次错误的关联 ID。
        log_id: String,
    },
}

impl StoreError {
    pub(crate) fn sqlite(source: rusqlite::Error) -> Self {
        let code = match source.sqlite_error_code() {
            Some(ErrorCode::DiskFull) => StorageErrorCode::Full,
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => StorageErrorCode::Corrupt,
            Some(ErrorCode::ReadOnly | ErrorCode::PermissionDenied) => StorageErrorCode::ReadOnly,
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => StorageErrorCode::Locked,
            _ => StorageErrorCode::Io,
        };
        Self::Sqlite {
            code,
            log_id: Uuid::now_v7().to_string(),
            source,
        }
    }

    pub(crate) fn file(source: io::Error) -> Self {
        let code = match source.kind() {
            io::ErrorKind::PermissionDenied => StorageErrorCode::ReadOnly,
            io::ErrorKind::StorageFull => StorageErrorCode::Full,
            io::ErrorKind::NotFound
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::NotADirectory => StorageErrorCode::PathInvalid,
            _ => StorageErrorCode::Io,
        };
        Self::File {
            code,
            log_id: Uuid::now_v7().to_string(),
            source,
        }
    }

    pub(crate) fn integrity() -> Self {
        Self::Integrity {
            log_id: Uuid::now_v7().to_string(),
        }
    }

    pub(crate) fn invalid(key: &'static str) -> Self {
        Self::InvalidSetting {
            key,
            log_id: Uuid::now_v7().to_string(),
        }
    }

    pub(crate) fn newer(found: i64, supported: i64) -> Self {
        Self::NewerSchema {
            found,
            supported,
            log_id: Uuid::now_v7().to_string(),
        }
    }

    /// 中文提示所需的稳定分类；更高 schema 由调用方另行提示升级。
    pub fn code(&self) -> StorageErrorCode {
        match self {
            Self::Sqlite { code, .. } | Self::File { code, .. } => *code,
            Self::Integrity { .. } => StorageErrorCode::Corrupt,
            Self::NewerSchema { .. } | Self::InvalidSetting { .. } => StorageErrorCode::Io,
        }
    }

    /// 可安全展示/记录的日志关联 ID。
    pub fn log_id(&self) -> &str {
        match self {
            Self::Sqlite { log_id, .. }
            | Self::File { log_id, .. }
            | Self::NewerSchema { log_id, .. }
            | Self::Integrity { log_id }
            | Self::InvalidSetting { log_id, .. } => log_id,
        }
    }

    /// 可安全记录的底层数字错误码，不记录 SQL 绑定值、路径或正文。
    pub fn native_code(&self) -> i32 {
        match self {
            Self::Sqlite { source, .. } => {
                source.sqlite_error().map_or(0, |error| error.extended_code)
            }
            Self::File { source, .. } => source.raw_os_error().unwrap_or(0),
            _ => 0,
        }
    }

    /// 诊断类别，配合日志 ID 和底层码定位，不将源错误字符串交给 UI。
    pub fn diagnostic_kind(&self) -> &'static str {
        match self {
            Self::Sqlite { .. } => "sqlite",
            Self::File { .. } => "file",
            Self::NewerSchema { .. } => "newer_schema",
            Self::Integrity { .. } => "integrity",
            Self::InvalidSetting { .. } => "invalid_setting",
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(source: rusqlite::Error) -> Self {
        Self::sqlite(source)
    }
}

impl From<io::Error> for StoreError {
    fn from(source: io::Error) -> Self {
        Self::file(source)
    }
}
