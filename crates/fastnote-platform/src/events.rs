use fastnote_core::EntryMode;

/// 入口菜单的意图，不包含可变业务状态或正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryAction {
    /// 显示当前主窗口路由。
    Show,
    /// 导航到一个固定模块，参数由应用控制器决定。
    Navigate(&'static str),
    /// 切换互斥入口。
    Switch(EntryMode),
    /// 完整退出。
    Quit,
}

/// 原生事件经有界通道交给 UI 事件循环，不在 Win32 回调里做业务操作。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeEvent {
    /// 左键点击的托盘 ID；应用层过滤旧入口残留事件。
    TrayShow(String),
    /// 菜单项 ID。
    Menu(String),
    /// 已按下的全局热键 ID。
    HotkeyShow(u32),
}
