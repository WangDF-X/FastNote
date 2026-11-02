use fastnote_core::StorageErrorCode;

/// ARCH-03 已登记存储错误的中文文案，不附带数据库/路径/正文详情。
pub fn storage_message(code: StorageErrorCode) -> &'static str {
    match code {
        StorageErrorCode::Full => "磁盘空间不足，无法保存。",
        StorageErrorCode::Corrupt => "数据文件异常，请从备份恢复。",
        StorageErrorCode::ReadOnly => "无法写入数据文件，请检查目录权限。",
        StorageErrorCode::Locked => "数据文件被其他程序占用，请关闭后重试。",
        StorageErrorCode::PathInvalid => "数据文件夹不可用，请检查路径和权限。",
        StorageErrorCode::Io => "读写数据文件失败，请重试。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_registered_storage_codes_have_safe_chinese_messages() {
        for code in [
            StorageErrorCode::Full,
            StorageErrorCode::Corrupt,
            StorageErrorCode::ReadOnly,
            StorageErrorCode::Locked,
            StorageErrorCode::PathInvalid,
            StorageErrorCode::Io,
        ] {
            assert!(storage_message(code).contains('。'));
            assert!(code.as_str().starts_with("E_STORAGE_"));
        }
    }
}
