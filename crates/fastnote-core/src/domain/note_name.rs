//! Windows 文件名的基础校验；目录内重名比较由仓储负责。

/// 文件名违反 Windows 文件管理器的常见命名限制。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoteNameError {
    /// 名称为空、全为空白，或末尾是空格或句点。
    EmptyOrTrailing,
    /// 名称包含 Windows 不允许的字符或控制字符。
    InvalidCharacter,
    /// 名称使用 Windows 保留的设备名。
    Reserved,
    /// 名称超过 Windows 常见单个路径组件的 255 个 UTF-16 代码单元。
    TooLong,
}

/// 校验单个文件或文件夹名，不裁剪或修改用户输入。
///
/// 大小写不敏感的重名判断需要与持久化层使用的 Windows 比较规则一致，
/// 不在这里使用与 Windows 不完全等价的 Unicode 小写转换代替。
pub fn validate_note_name(name: &str) -> Result<(), NoteNameError> {
    if name.trim().is_empty() || name.ends_with([' ', '.']) {
        return Err(NoteNameError::EmptyOrTrailing);
    }
    if name
        .chars()
        .any(|c| c <= '\u{1f}' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return Err(NoteNameError::InvalidCharacter);
    }
    if name.encode_utf16().count() > 255 {
        return Err(NoteNameError::TooLong);
    }
    let stem = name.split('.').next().unwrap_or_default().trim_end();
    let upper = stem.to_ascii_uppercase();
    let reserved_numbered = ["COM", "LPT"].iter().any(|prefix| {
        upper.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") || reserved_numbered {
        return Err(NoteNameError::Reserved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_rejects_windows_invalid_and_reserved_names() {
        for name in ["", "   ", "note.", "note "] {
            assert_eq!(
                validate_note_name(name),
                Err(NoteNameError::EmptyOrTrailing)
            );
        }
        for name in ["a/b", "a\\b", "a:b", "a?b", "a\nb"] {
            assert_eq!(
                validate_note_name(name),
                Err(NoteNameError::InvalidCharacter)
            );
        }
        for name in ["con", "NUL.txt", "com1.md", "LPT³.txt", "aux .md"] {
            assert_eq!(validate_note_name(name), Err(NoteNameError::Reserved));
        }
        for name in ["随手记.md", "COM0", "file.name", "📁笔记"] {
            assert_eq!(validate_note_name(name), Ok(()));
        }
    }

    #[test]
    fn test_name_length_counts_utf16_units_not_unicode_scalars() {
        assert!(validate_note_name(&"a".repeat(255)).is_ok());
        assert_eq!(
            validate_note_name(&"a".repeat(256)),
            Err(NoteNameError::TooLong)
        );
        assert!(validate_note_name(&"😀".repeat(127)).is_ok());
        assert_eq!(
            validate_note_name(&"😀".repeat(128)),
            Err(NoteNameError::TooLong)
        );
    }
}
