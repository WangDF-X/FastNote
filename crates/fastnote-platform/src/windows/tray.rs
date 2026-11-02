use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use crossbeam_channel::{Sender, TrySendError};
use fastnote_core::EntryMode;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::{EntryAction, NativeEvent};

use super::PlatformError;

/// 一次安装原生回调。库的 OnceCell 不可重设；回调仅保留 Sender 与安全唤醒函数。
/// 当前外壳不创建原生 Slint MenuBar，muda 事件由此入口统一拥有。
pub fn install_event_handlers(sender: Sender<NativeEvent>, wake: Arc<dyn Fn() + Send + Sync>) {
    let dispatch = Arc::new(move |event| match sender.try_send(event) {
        Ok(()) => wake(),
        Err(TrySendError::Full(_)) => {
            tracing::warn!("native entry queue full, newest event dropped")
        }
        Err(TrySendError::Disconnected(_)) => {}
    });
    TrayIconEvent::set_event_handler(Some({
        let dispatch = dispatch.clone();
        move |event| match event {
            TrayIconEvent::Click {
                id,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                id,
                button: MouseButton::Left,
                ..
            } => {
                dispatch(NativeEvent::TrayShow(id.0));
            }
            _ => {}
        }
    }));
    MenuEvent::set_event_handler(Some({
        let dispatch = dispatch.clone();
        move |event: MenuEvent| dispatch(NativeEvent::Menu(event.id.0))
    }));
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            dispatch(NativeEvent::HotkeyShow(event.id));
        }
    }));
}

/// 托盘图标及固定菜单，在 UI 线程拥有；Drop 会移除图标。
pub struct WindowsTray {
    icon: TrayIcon,
    actions: HashMap<String, EntryAction>,
}

impl WindowsTray {
    /// 使用已有品牌位图创建托盘。库负责 TaskbarCreated 后重建 Explorer 图标。
    pub fn new() -> Result<Self, PlatformError> {
        let menu = Menu::new();
        let mut actions = HashMap::new();
        let mut append = |label: &str, action| -> Result<(), PlatformError> {
            let item = MenuItem::new(label, true, None);
            menu.append(&item)?;
            actions.insert(item.id().0.clone(), action);
            Ok(())
        };
        append("打开主界面", EntryAction::Show)?;
        menu.append(&PredefinedMenuItem::separator())?;
        for page in ["今日任务", "日历", "小记", "日记", "计划板"] {
            append(page, EntryAction::Navigate(page))?;
        }
        menu.append(&PredefinedMenuItem::separator())?;
        append("设置", EntryAction::Navigate("设置"))?;
        append("切换到悬浮球", EntryAction::Switch(EntryMode::Ball))?;
        menu.append(&PredefinedMenuItem::separator())?;
        append("退出 FastNote", EntryAction::Quit)?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("FastNote")
            .with_icon(brand_icon()?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        Ok(Self { icon, actions })
    }

    /// 是否为当前托盘的点击。
    pub fn matches(&self, id: &str) -> bool {
        self.icon.id().as_ref() == id
    }

    /// 解析当前菜单项，不接受已销毁入口的旧菜单事件。
    pub fn menu_action(&self, id: &str) -> Option<EntryAction> {
        self.actions.get(id).copied()
    }

    /// 切换时先隐藏旧入口，目标入口成功后再销毁旧句柄，失败可恢复。
    pub fn set_visible(&self, visible: bool) -> Result<(), PlatformError> {
        self.icon.set_visible(visible)?;
        Ok(())
    }
}

fn brand_icon() -> Result<Icon, PlatformError> {
    const PNG: &[u8] = include_bytes!("../../../../ui/branding/png/fastnote-32.png");
    let mut decoder = png::Decoder::new(Cursor::new(PNG));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| PlatformError::Icon)?;
    let mut bytes = vec![0; reader.output_buffer_size().ok_or(PlatformError::Icon)?];
    let info = reader
        .next_frame(&mut bytes)
        .map_err(|_| PlatformError::Icon)?;
    bytes.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes,
        png::ColorType::Rgb => bytes
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err(PlatformError::Icon),
    };
    Icon::from_rgba(rgba, info.width, info.height).map_err(|_| PlatformError::Icon)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_committed_brand_png_decodes_as_a_valid_tray_icon() {
        assert!(super::brand_icon().is_ok());
    }
}
