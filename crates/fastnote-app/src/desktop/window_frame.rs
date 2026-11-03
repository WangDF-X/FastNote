use fastnote_ui::ShellPalette;
use slint::winit_030::winit::{
    platform::windows::{Color as NativeColor, CornerPreference, WindowExtWindows},
    window::Window,
};
use slint::{Color, ComponentHandle};

use super::runtime::Runtime;
use super::window::native_handle;

/// D-SHELL-01：UI 线程中的视觉去重快照，不持有窗口或业务对象。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowFrameState {
    handle: isize,
    maximized: bool,
    border: Color,
}

impl WindowFrameState {
    fn corners(self) -> CornerPreference {
        if self.maximized {
            CornerPreference::DoNotRound
        } else {
            CornerPreference::Round
        }
    }

    fn native_border(self) -> Option<NativeColor> {
        if self.maximized {
            None
        } else {
            Some(NativeColor::from_rgb(
                self.border.red(),
                self.border.green(),
                self.border.blue(),
            ))
        }
    }
}

impl Runtime {
    /// 复用 winit 安全接口配置真实窗口圆角/外沿；不扩大客户区或另建阴影窗口。
    pub(super) fn apply_main_frame(&self, window: &Window) {
        let Some(handle) = native_handle(window) else {
            return;
        };
        let state = WindowFrameState {
            handle: handle.0,
            maximized: window.is_maximized(),
            border: self
                .shell
                .component()
                .global::<ShellPalette>()
                .get_border_strong(),
        };
        if self.main_frame.get() == Some(state) {
            return;
        }
        // Windows 11 原生裁切整个窗口；旧系统的安全接口为无效果降级，UI 细边缘仍保留。
        // 阴影沿用创建时的 undecorated_shadow，最大化的收起由系统处理，不改窗口尺寸。
        window.set_corner_preference(state.corners());
        window.set_border_color(state.native_border());
        self.main_frame.set(Some(state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(maximized: bool, border: Color) -> WindowFrameState {
        WindowFrameState {
            handle: 1,
            maximized,
            border,
        }
    }

    #[test]
    fn test_windowed_frame_requests_round_corners_and_preserves_rgb_channel_order() {
        for (red, green, blue) in [(0, 0, 0), (255, 255, 255), (201, 207, 214), (58, 65, 74)] {
            let state = frame(false, Color::from_rgb_u8(red, green, blue));
            assert_eq!(state.corners(), CornerPreference::Round);
            assert_eq!(
                state.native_border(),
                Some(NativeColor::from_rgb(red, green, blue))
            );
        }
    }

    #[test]
    fn test_maximized_frame_removes_corner_rounding_and_native_outline() {
        let state = frame(true, Color::from_rgb_u8(201, 207, 214));
        assert_eq!(state.corners(), CornerPreference::DoNotRound);
        assert_eq!(state.native_border(), None);
        let restored = WindowFrameState {
            maximized: false,
            ..state
        };
        assert_eq!(restored.corners(), CornerPreference::Round);
        assert_eq!(
            restored.native_border(),
            Some(NativeColor::from_rgb(201, 207, 214))
        );
    }

    #[test]
    fn test_frame_cache_changes_for_handle_theme_and_maximize_but_not_repeated_resize() {
        let original = frame(false, Color::from_rgb_u8(201, 207, 214));
        let cached = Some(original);
        let repeated = frame(false, Color::from_rgb_u8(201, 207, 214));
        assert_eq!(cached, Some(repeated));
        for changed in [
            WindowFrameState {
                handle: 2,
                ..original
            },
            WindowFrameState {
                maximized: true,
                ..original
            },
            WindowFrameState {
                border: Color::from_rgb_u8(58, 65, 74),
                ..original
            },
        ] {
            assert_ne!(cached, Some(changed));
        }
        assert_ne!(None, Some(original));
    }

    #[test]
    fn test_frame_is_reapplied_for_native_window_recreation_theme_and_state_changes() {
        // 源码接线检查；不创建窗口或模拟最大化/缩放。
        let creation = include_str!("mod.rs");
        let main = creation.split_once("\"FastNote\" => attributes").unwrap().1;
        let main = main.split_once("_ => attributes").unwrap().0;
        assert!(main.contains(".with_undecorated_shadow(true)"));
        assert!(main.contains(".with_corner_preference(CornerPreference::Round)"));
        let show = include_str!("presentation/mod.rs");
        let recreating = show.split_once("if recreating {").unwrap().1;
        assert!(recreating
            .split_once("self.shell.show()")
            .unwrap()
            .0
            .contains("self.main_frame.set(None);"));
        assert!(include_str!("theme.rs").contains("self.apply_main_frame(window);"));
        assert!(include_str!("window.rs").contains("self.apply_main_frame(window);"));
    }
}
