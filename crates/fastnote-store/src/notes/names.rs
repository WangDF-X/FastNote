//! 与 Windows 的 ordinal 忽略大小写比较保持一致，避免 Unicode 小写映射误判。

#[cfg(windows)]
fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    use windows::Win32::Globalization::{CompareStringOrdinal, CSTR_GREATER_THAN, CSTR_LESS_THAN};
    let left: Vec<u16> = left.encode_utf16().collect();
    let right: Vec<u16> = right.encode_utf16().collect();
    // SAFETY: 两个 slice 在调用期间有效，API 按 slice 长度只读，不持有指针。
    let result = unsafe { CompareStringOrdinal(&left, &right, true) };
    // 系统调用失败时保守地阻止写入，不能允许可能重名的数据覆盖。
    if result == CSTR_LESS_THAN {
        std::cmp::Ordering::Less
    } else if result == CSTR_GREATER_THAN {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

#[cfg(not(windows))]
fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

pub(crate) fn same_name(left: &str, right: &str) -> bool {
    compare(left, right).is_eq()
}

pub(crate) fn capture_base(content: &str) -> String {
    let (title, _) = fastnote_core::note_excerpt(content);
    let clean: String = title
        .chars()
        .map(|c| {
            if c <= '\u{1f}' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    if fastnote_core::domain::validate_note_name(clean).is_ok() {
        clean.to_owned()
    } else {
        "小记".to_owned()
    }
}

pub(crate) fn numbered(base: &str, number: u32) -> String {
    let suffix = format!(" ({number})");
    let mut units = 255 - suffix.encode_utf16().count();
    let prefix: String = base
        .chars()
        .take_while(|c| {
            if c.len_utf16() > units {
                false
            } else {
                units -= c.len_utf16();
                true
            }
        })
        .collect();
    format!("{}{}", prefix.trim_end_matches([' ', '.']), suffix)
}

#[derive(Debug, Clone)]
pub(crate) struct NameKey(pub(crate) String);
impl PartialEq for NameKey {
    fn eq(&self, other: &Self) -> bool {
        compare(&self.0, &other.0).is_eq()
    }
}
impl Eq for NameKey {}
impl PartialOrd for NameKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NameKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        compare(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::same_name;

    #[test]
    fn test_names_compare_case_insensitively() {
        assert!(same_name("AbC.txt", "aBc.TXT"));
        assert!(same_name("笔记", "笔记"));
        assert!(!same_name("甲", "乙"));
    }
}
