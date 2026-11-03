use thiserror::Error;

/// 平台错误保留源错误；界面仅按具体功能显示固定中文提示。
#[derive(Debug, Error)]
pub enum PlatformError {
    /// Windows 原生能力失败。
    #[error("Windows API error")]
    Native(#[from] ::windows::core::Error),
    /// 托盘句柄创建失败。
    #[error("Tray creation error")]
    Tray(#[from] tray_icon::Error),
    /// 菜单创建失败。
    #[error("Tray menu error")]
    Menu(#[from] tray_icon::menu::Error),
    /// 热键管理/注册失败。
    #[error("Global hotkey error")]
    Hotkey(#[from] global_hotkey::Error),
    /// 热键格式不合法。
    #[error("Invalid global hotkey")]
    InvalidHotkey,
    /// 编译期嵌入的品牌图标无法解码。
    #[error("Invalid brand icon")]
    Icon,
    /// 系统应用主题不是有效的 0/1 DWORD。
    #[error("Invalid Windows application theme")]
    InvalidTheme,
}

impl PlatformError {
    /// 不含用户内容的底层 Windows HRESULT，非 Windows 失败为 0。
    pub fn native_code(&self) -> i32 {
        match self {
            Self::Native(error) => error.code().0,
            _ => 0,
        }
    }

    /// 安全的诊断分类；不是产品/业务错误码。
    pub fn diagnostic_kind(&self) -> &'static str {
        match self {
            Self::Native(_) => "windows",
            Self::Tray(_) => "tray",
            Self::Menu(_) => "menu",
            Self::Hotkey(_) => "hotkey",
            Self::InvalidHotkey => "invalid_hotkey",
            Self::Icon => "brand_icon",
            Self::InvalidTheme => "system_theme",
        }
    }
}
