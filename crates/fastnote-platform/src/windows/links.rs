use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn allowed_web_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let remainder = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    remainder.is_some_and(|value| {
        !value.is_empty()
            && !value.starts_with(['/', '?', '#', ':'])
            && !value
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    })
}

/// 只打开用户主动点击的 HTTP(S) 链接；不通过 shell 命令解释用户内容。
pub fn open_web_link(url: &str) -> bool {
    if !allowed_web_link(url) {
        return false;
    }
    let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: 宽字符串以 NUL 结束并在调用期间有效；不传入命令行或工作目录。
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_web_urls_are_allowed() {
        for url in ["https://example.org/a", "HTTP://example.org"] {
            assert!(allowed_web_link(url));
        }
        for url in [
            "file:///a",
            "javascript:alert(1)",
            "https://",
            "https:///x",
            "https://a b",
            "https://a\\b",
            "https://a\0b",
        ] {
            assert!(!allowed_web_link(url));
        }
    }
}
