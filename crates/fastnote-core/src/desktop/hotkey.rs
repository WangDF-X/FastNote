/// ARCH-04 §7.2 配置格式检查，空组合合法（禁用）；不做系统注册或冲突检测。
pub fn is_valid_hotkey(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let parts: Vec<_> = value.split('+').collect();
    let Some(key) = parts.last() else {
        return false;
    };
    let mut previous = 0;
    for modifier in &parts[..parts.len() - 1] {
        let index = match *modifier {
            "Ctrl" => 1,
            "Alt" => 2,
            "Shift" => 3,
            "Super" => 4,
            _ => return false,
        };
        if index <= previous {
            return false;
        }
        previous = index;
    }
    *key == "Space"
        || (key.len() == 1
            && key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
        || key
            .strip_prefix('F')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=12).contains(&number) && *key == format!("F{number}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_hotkey_format_accepts_empty_and_supported_keys() {
        for value in [
            "",
            "Ctrl+Alt+Space",
            "Ctrl+Alt+Shift+Super+A",
            "Alt+9",
            "Ctrl+F1",
            "Ctrl+F12",
        ] {
            assert!(is_valid_hotkey(value), "{value}");
        }
        for value in [
            "Ctrl+",
            "Ctrl+Ctrl+A",
            "Alt+Ctrl+A",
            "Meta+A",
            "ctrl+A",
            "Ctrl+F0",
            "Ctrl+F13",
            "Ctrl+F01",
            "Ctrl+a",
            "Ctrl+Enter",
        ] {
            assert!(!is_valid_hotkey(value), "{value}");
        }
    }
}
