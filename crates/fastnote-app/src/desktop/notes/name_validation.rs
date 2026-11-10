use fastnote_core::domain::{validate_note_name, NoteNameError};

/// 保持领域校验为唯一规则来源，仅将错误转换为用户可执行的提示。
pub(super) fn name_error(name: &str) -> Option<String> {
    match validate_note_name(name) {
        Ok(()) => None,
        Err(NoteNameError::EmptyOrTrailing) if name.trim().is_empty() => {
            Some("名称不能为空或全为空白。".into())
        }
        Err(NoteNameError::EmptyOrTrailing) if name.ends_with(' ') => {
            Some("名称末尾不能是空格。".into())
        }
        Err(NoteNameError::EmptyOrTrailing) => Some("名称末尾不能是句点（.）。".into()),
        Err(NoteNameError::InvalidCharacter) => {
            let invalid = name.chars().find(|c| {
                *c <= '\u{1f}' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            });
            Some(match invalid {
                Some(c) if c <= '\u{1f}' => "名称不能包含换行、制表符等控制字符。".into(),
                Some(c) => format!("名称不能包含字符“{c}”。"),
                None => "名称包含不允许的字符。".into(),
            })
        }
        Err(NoteNameError::Reserved) => {
            Some("名称使用了 Windows 保留设备名（如 CON、NUL、COM1），请更换。".into())
        }
        Err(NoteNameError::TooLong) => {
            Some("名称不能超过 255 个 UTF-16 单元；部分字符占两个单元。".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_name_errors_cover_all_validation_categories() {
        assert_eq!(name_error(""), Some("名称不能为空或全为空白。".into()));
        assert_eq!(name_error("  "), Some("名称不能为空或全为空白。".into()));
        assert_eq!(name_error("名称 "), Some("名称末尾不能是空格。".into()));
        assert_eq!(
            name_error("名称."),
            Some("名称末尾不能是句点（.）。".into())
        );
        for character in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
            assert_eq!(
                name_error(&format!("a{character}b")),
                Some(format!("名称不能包含字符“{character}”。"))
            );
        }
        assert_eq!(
            name_error("a\nb"),
            Some("名称不能包含换行、制表符等控制字符。".into())
        );
        assert!(name_error("CON.txt").unwrap().contains("保留设备名"));
        assert!(name_error(&"😀".repeat(128)).unwrap().contains("255"));
        assert_eq!(name_error("有效名称"), None);
    }
}
