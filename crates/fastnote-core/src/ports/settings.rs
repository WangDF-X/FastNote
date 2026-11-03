use crate::{DateTime, DesktopSetting, DesktopSettings, Utc};

/// 设置仓储边界；连接和事务只由存储层/工作线程拥有。
pub trait SettingsRepository {
    /// 实现提供的可追溯错误，不要求 core 依赖 IO 库。
    type Error: std::error::Error;

    /// 读取并校验桌面与今日偏好配置，缺失/非法值按 ARCH-04 回退默认。
    fn load_desktop(&self) -> Result<DesktopSettings, Self::Error>;

    /// 一个批次一个 IMMEDIATE 事务，失败时整批回滚。
    fn save_desktop(
        &mut self,
        changes: &[DesktopSetting],
        now: DateTime<Utc>,
    ) -> Result<(), Self::Error>;
}

/// ARCH-03 §10 已登记的存储错误码，UI 只映射这些分类，不展示内部详情。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageErrorCode {
    /// 磁盘空间不足。
    Full,
    /// 数据文件损坏或完整性异常。
    Corrupt,
    /// 只读或无权限写入。
    ReadOnly,
    /// 被外部程序占用。
    Locked,
    /// 目录非法或无法创建。
    PathInvalid,
    /// 其它读写错误。
    Io,
}

impl StorageErrorCode {
    /// 已登记且不可变更的代码文本。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "E_STORAGE_FULL",
            Self::Corrupt => "E_STORAGE_CORRUPT",
            Self::ReadOnly => "E_STORAGE_READONLY",
            Self::Locked => "E_STORAGE_LOCKED",
            Self::PathInvalid => "E_STORAGE_PATH_INVALID",
            Self::Io => "E_STORAGE_IO",
        }
    }
}
