use serde::{Deserialize, Serialize};

use crate::{derive::calendar::WeekStart, TodayPreferences, TodaySort};

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

/// ARCH-05 §7：主题三态；`system` 跟随 Windows 应用主题设置。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// 跟随系统。
    #[default]
    System,
    /// 始终浅色。
    Light,
    /// 始终深色。
    Dark,
}

impl ThemeMode {
    /// 稳定配置值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    /// 严格解析配置值，未知值回退由调用方决定。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
    /// 结合系统状态解析实际是否使用深色调色板。
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Self::System => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }

    /// SET-02 §3.2：单图标按钮的下一模式，按系统 → 浅色 → 深色 → 系统循环。
    pub fn next_mode(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }
}

/// 由设置仓储读取、应用层拥有的桌面与今日偏好已确认快照。
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopSettings {
    /// 用户选择的入口模式。
    pub entry_mode: EntryMode,
    /// 主题选择。
    pub theme: ThemeMode,
    /// 日历每周的起始日。
    pub week_start: WeekStart,
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
    /// 今日排序/显示偏好；与任务查询读取相同的规范配置键。
    pub today_preferences: TodayPreferences,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            entry_mode: EntryMode::Ball,
            theme: ThemeMode::System,
            week_start: WeekStart::default(),
            close_behavior: CloseBehavior::Entry,
            hotkeys_enabled: true,
            show_hotkey: "Ctrl+Alt+Space".to_owned(),
            geometry: None,
            ball_position: None,
            first_run_done: false,
            today_preferences: TodayPreferences::default(),
        }
    }
}

/// 本阶段允许写入的配置，键均来自 ARCH-04。
#[derive(Clone, Debug, PartialEq)]
pub enum DesktopSetting {
    /// 入口形态。
    EntryMode(EntryMode),
    /// 主题三态。
    Theme(ThemeMode),
    /// 日历每周的起始日。
    WeekStart(WeekStart),
    /// 主窗口关闭策略。
    CloseBehavior(CloseBehavior),
    /// 主窗口几何；None 清除旧值。
    MainGeometry(Option<WindowGeometry>),
    /// 悬浮球位置；None 清除旧值。
    BallPosition(Option<BallPosition>),
    /// 首次启动标记。
    FirstRunDone(bool),
    /// 当前关注区的排序，始终保持逾期置顶。
    TodaySort(TodaySort),
    /// 显示待安排分组。
    TodayShowUnscheduled(bool),
    /// 显示今日已完成分组。
    TodayShowCompleted(bool),
}

impl DesktopSetting {
    /// 已登记的稳定配置键。
    pub fn key(&self) -> &'static str {
        match self {
            Self::EntryMode(_) => "general.entry_mode",
            Self::Theme(_) => "appearance.theme",
            Self::WeekStart(_) => "appearance.week_starts_on",
            Self::CloseBehavior(_) => "general.close_behavior",
            Self::MainGeometry(_) => "window.main_geometry",
            Self::BallPosition(_) => "appearance.ball_position",
            Self::FirstRunDone(_) => "general.first_run_done",
            Self::TodaySort(_) => "tasks.default_sort",
            Self::TodayShowUnscheduled(_) => "today.show_unscheduled",
            Self::TodayShowCompleted(_) => "today.show_completed",
        }
    }
}

impl DesktopSettings {
    /// 在仓储提交成功后更新应用层的快照，失败时不调用。
    pub fn apply(&mut self, change: &DesktopSetting) {
        match change {
            DesktopSetting::EntryMode(value) => self.entry_mode = *value,
            DesktopSetting::Theme(value) => self.theme = *value,
            DesktopSetting::WeekStart(value) => self.week_start = *value,
            DesktopSetting::CloseBehavior(value) => self.close_behavior = *value,
            DesktopSetting::MainGeometry(value) => self.geometry = *value,
            DesktopSetting::BallPosition(value) => self.ball_position = value.clone(),
            DesktopSetting::FirstRunDone(value) => self.first_run_done = *value,
            DesktopSetting::TodaySort(value) => self.today_preferences.sort = *value,
            DesktopSetting::TodayShowUnscheduled(value) => {
                self.today_preferences.show_unscheduled = *value
            }
            DesktopSetting::TodayShowCompleted(value) => {
                self.today_preferences.show_completed = *value
            }
        }
    }
}

#[cfg(test)]
mod task_preferences_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_arch_04() {
        let settings = DesktopSettings::default();
        assert_eq!(settings.entry_mode, EntryMode::Ball);
        assert_eq!(settings.theme, ThemeMode::System);
        assert_eq!(settings.close_behavior, CloseBehavior::Entry);
        assert_eq!(settings.show_hotkey, "Ctrl+Alt+Space");
        assert!(!settings.first_run_done);
        assert_eq!(settings.today_preferences, TodayPreferences::default());
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
    fn test_theme_mode_round_trips_and_resolves_system_state() {
        for value in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::parse(value.as_str()), Some(value));
        }
        assert_eq!(ThemeMode::parse("auto"), None);
        assert!(ThemeMode::System.is_dark(true));
        assert!(!ThemeMode::System.is_dark(false));
        assert!(!ThemeMode::Light.is_dark(true));
        assert!(!ThemeMode::Light.is_dark(false));
        assert!(ThemeMode::Dark.is_dark(false));
        assert!(ThemeMode::Dark.is_dark(true));
    }
    #[test]
    fn test_theme_button_cycles_all_three_modes_and_wraps() {
        let mut mode = ThemeMode::System;
        for expected in [
            ThemeMode::Light,
            ThemeMode::Dark,
            ThemeMode::System,
            ThemeMode::Light,
            ThemeMode::Dark,
            ThemeMode::System,
        ] {
            mode = mode.next_mode();
            assert_eq!(mode, expected);
        }
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_ne!(mode.next_mode(), mode);
            assert_eq!(mode.next_mode().next_mode().next_mode(), mode);
        }
    }

    #[test]
    fn test_committed_changes_only_touch_their_own_field() {
        let mut settings = DesktopSettings::default();
        settings.apply(&DesktopSetting::EntryMode(EntryMode::Tray));
        settings.apply(&DesktopSetting::Theme(ThemeMode::Dark));
        settings.apply(&DesktopSetting::CloseBehavior(CloseBehavior::Quit));
        settings.apply(&DesktopSetting::FirstRunDone(true));
        assert_eq!(settings.entry_mode, EntryMode::Tray);
        assert_eq!(settings.theme, ThemeMode::Dark);
        assert_eq!(settings.close_behavior, CloseBehavior::Quit);
        assert!(settings.first_run_done);
        assert_eq!(settings.geometry, None);
    }
}
