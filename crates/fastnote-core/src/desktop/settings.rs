use serde::{Deserialize, Serialize};

use super::{BallPosition, WindowGeometry};

/// ARCH-04 §7.1：常驻入口形态，两者互斥。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryMode {
    /// 桌面悬浮球。
    #[default]
    Ball,
    /// Windows 通知区域托盘。
    Tray,
}

impl EntryMode {
    /// 稳定配置值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ball => "ball",
            Self::Tray => "tray",
        }
    }
    /// 严格解析配置值，不接受其它模式。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ball" => Some(Self::Ball),
            "tray" => Some(Self::Tray),
            _ => None,
        }
    }
}

/// ARCH-04 §7.1：关闭主窗口时的策略。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CloseBehavior {
    /// 隐藏主窗口，保留常驻入口。
    #[default]
    Entry,
    /// 完整退出应用。
    Quit,
}

impl CloseBehavior {
    /// 稳定配置值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Quit => "quit",
        }
    }
    /// 严格解析配置值。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "entry" => Some(Self::Entry),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }
}

/// 由设置仓储读取、应用层拥有的桌面配置快照。
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopSettings {
    /// 用户选择的入口模式。
    pub entry_mode: EntryMode,
    /// 关闭窗口策略。
    pub close_behavior: CloseBehavior,
    /// 是否注册全局热键。
    pub hotkeys_enabled: bool,
    /// 唤起组合，空字符串表示不注册。
    pub show_hotkey: String,
    /// 主窗口还原矩形及最大化标记。
    pub geometry: Option<WindowGeometry>,
    /// 悬浮球工具窗口位置。
    pub ball_position: Option<BallPosition>,
    /// 首次手动启动已完成。
    pub first_run_done: bool,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            entry_mode: EntryMode::Ball,
            close_behavior: CloseBehavior::Entry,
            hotkeys_enabled: true,
            show_hotkey: "Ctrl+Alt+Space".to_owned(),
            geometry: None,
            ball_position: None,
            first_run_done: false,
        }
    }
}

/// 本阶段允许写入的配置，键均来自 ARCH-04。
#[derive(Clone, Debug, PartialEq)]
pub enum DesktopSetting {
    /// 入口形态。
    EntryMode(EntryMode),
    /// 主窗口关闭策略。
    CloseBehavior(CloseBehavior),
    /// 主窗口几何；None 清除旧值。
    MainGeometry(Option<WindowGeometry>),
    /// 悬浮球位置；None 清除旧值。
    BallPosition(Option<BallPosition>),
    /// 首次启动标记。
    FirstRunDone(bool),
}

impl DesktopSetting {
    /// 已登记的稳定配置键。
    pub fn key(&self) -> &'static str {
        match self {
            Self::EntryMode(_) => "general.entry_mode",
            Self::CloseBehavior(_) => "general.close_behavior",
            Self::MainGeometry(_) => "window.main_geometry",
            Self::BallPosition(_) => "appearance.ball_position",
            Self::FirstRunDone(_) => "general.first_run_done",
        }
    }
}

impl DesktopSettings {
    /// 在仓储提交成功后更新应用层的快照，失败时不调用。
    pub fn apply(&mut self, change: &DesktopSetting) {
        match change {
            DesktopSetting::EntryMode(value) => self.entry_mode = *value,
            DesktopSetting::CloseBehavior(value) => self.close_behavior = *value,
            DesktopSetting::MainGeometry(value) => self.geometry = *value,
            DesktopSetting::BallPosition(value) => self.ball_position = value.clone(),
            DesktopSetting::FirstRunDone(value) => self.first_run_done = *value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_arch_04() {
        let settings = DesktopSettings::default();
        assert_eq!(settings.entry_mode, EntryMode::Ball);
        assert_eq!(settings.close_behavior, CloseBehavior::Entry);
        assert_eq!(settings.show_hotkey, "Ctrl+Alt+Space");
        assert!(!settings.first_run_done);
    }
    #[test]
    fn test_enum_values_round_trip_without_unknown_fallback() {
        for value in [EntryMode::Ball, EntryMode::Tray] {
            assert_eq!(EntryMode::parse(value.as_str()), Some(value));
        }
        for value in [CloseBehavior::Entry, CloseBehavior::Quit] {
            assert_eq!(CloseBehavior::parse(value.as_str()), Some(value));
        }
        assert_eq!(EntryMode::parse("none"), None);
        assert_eq!(CloseBehavior::parse("invalid"), None);
    }
    #[test]
    fn test_committed_changes_only_touch_their_own_field() {
        let mut settings = DesktopSettings::default();
        settings.apply(&DesktopSetting::EntryMode(EntryMode::Tray));
        settings.apply(&DesktopSetting::CloseBehavior(CloseBehavior::Quit));
        settings.apply(&DesktopSetting::FirstRunDone(true));
        assert_eq!(settings.entry_mode, EntryMode::Tray);
        assert_eq!(settings.close_behavior, CloseBehavior::Quit);
        assert!(settings.first_run_done);
        assert_eq!(settings.geometry, None);
    }
}
