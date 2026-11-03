use fastnote_core::{DesktopSetting, ThemeMode};
use slint::winit_030::{winit::window::Theme, WinitWindowAccessor};
use slint::ComponentHandle;

use super::runtime::Runtime;
use super::window::native_handle;

/// 只在 UI 线程持有的纯状态：已提交偏好留在 DesktopSettings，预览可独立回滚。
pub(super) struct ThemeState {
    preview: Option<ThemeMode>,
    system_dark: bool,
    system_available: bool,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self {
            preview: None,
            system_dark: false,
            system_available: true,
        }
    }
}

impl ThemeState {
    fn selected(&self, committed: ThemeMode) -> ThemeMode {
        self.preview.unwrap_or(committed)
    }

    fn dark(&self, committed: ThemeMode) -> bool {
        self.selected(committed).is_dark(self.system_dark)
    }

    fn system_changed(&mut self, value: Option<bool>) {
        self.system_dark = value.unwrap_or(false);
        self.system_available = value.is_some();
    }

    fn settle(&mut self, changes: &[DesktopSetting]) {
        if changes
            .iter()
            .any(|change| matches!(change, DesktopSetting::Theme(_)))
        {
            self.preview = None;
        }
    }
}

impl Runtime {
    pub(super) fn refresh_system_theme(&self) {
        let result = fastnote_platform::system_theme_is_dark();
        let mut state = self.theme.borrow_mut();
        if let Err(error) = &result {
            if state.system_available {
                tracing::warn!(
                    phase = "system_theme_read",
                    error_kind = error.diagnostic_kind(),
                    native_code = error.native_code(),
                    "system theme unavailable, falling back to light"
                );
            }
        }
        state.system_changed(result.ok());
        drop(state);
        self.sync_theme();
    }

    pub(super) fn preview_theme(&self, mode: ThemeMode) {
        self.theme.borrow_mut().preview = Some(mode);
        if mode == ThemeMode::System {
            // 手动模式期间也可能发生系统变化，回到 system 时不要沿用旧快照。
            self.refresh_system_theme();
        } else {
            self.sync_theme();
        }
    }

    pub(super) fn settle_theme(&self, changes: &[DesktopSetting]) {
        self.theme.borrow_mut().settle(changes);
    }

    pub(super) fn sync_theme(&self) {
        let committed = self.settings.borrow().theme;
        let state = self.theme.borrow();
        let dark = state.dark(committed);
        let main = self.shell.component();
        main.set_theme_mode(state.selected(committed).as_str().into());
        main.set_system_dark(state.system_dark);
        main.set_system_theme_available(state.system_available);
        fastnote_ui::apply_theme(main, &self.ball, &self.menu, dark);
        drop(state);
        // 原生回退标题栏以及无边框阴影也尽量匹配；不保留底层 winit Arc。
        main.window().with_winit_window(|window| {
            window.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }));
            self.apply_main_frame(window);
        });
    }

    pub(super) fn apply_main_caption(&self) {
        let main = self.shell.component();
        if !main.get_custom_titlebar() {
            return;
        }
        let result = main.window().with_winit_window(|window| {
            let handle = native_handle(window).ok_or(())?;
            if self.main_caption.get() == handle.0 {
                return Ok(());
            }
            fastnote_platform::configure_custom_caption(handle).map_err(|error| {
                tracing::warn!(phase = "custom_caption", native_code = error.native_code());
            })?;
            self.main_caption.set(handle.0);
            Ok(())
        });
        if matches!(result, Some(Err(()))) {
            // 不让不可拖动的窗口交给用户；启用原生框架并移除自绘标题栏。
            main.set_custom_titlebar(false);
            self.sync_theme();
            self.notice(
                fastnote_ui::ToastKind::Warning,
                "自绘标题栏暂不可用，已使用系统标题栏。",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_theme_ignores_system_changes_but_system_mode_follows() {
        let mut state = ThemeState::default();
        for system in [true, false, true] {
            state.system_changed(Some(system));
            assert_eq!(state.dark(ThemeMode::System), system);
            assert!(state.dark(ThemeMode::Dark));
            assert!(!state.dark(ThemeMode::Light));
        }
    }

    #[test]
    fn test_preview_is_immediate_and_unrelated_saves_do_not_clear_it() {
        let mut state = ThemeState {
            preview: Some(ThemeMode::Dark),
            ..Default::default()
        };
        assert!(state.dark(ThemeMode::Light));
        assert_eq!(state.selected(ThemeMode::Light), ThemeMode::Dark);
        state.settle(&[DesktopSetting::FirstRunDone(true)]);
        assert!(state.dark(ThemeMode::Light));
        state.settle(&[DesktopSetting::Theme(ThemeMode::Dark)]);
        assert_eq!(state.selected(ThemeMode::Dark), ThemeMode::Dark);
        assert_eq!(state.preview, None);
    }

    #[test]
    fn test_failed_preview_restores_committed_mode_with_current_system_state() {
        let mut state = ThemeState {
            preview: Some(ThemeMode::Light),
            ..Default::default()
        };
        state.system_changed(Some(true));
        assert!(!state.dark(ThemeMode::System));
        state.settle(&[DesktopSetting::Theme(ThemeMode::Light)]);
        assert!(state.dark(ThemeMode::System));
        assert_eq!(state.selected(ThemeMode::System), ThemeMode::System);
    }

    #[test]
    fn test_read_failure_falls_back_to_light_without_overriding_manual_dark() {
        let mut state = ThemeState::default();
        state.system_changed(Some(true));
        state.system_changed(None);
        assert!(!state.system_available);
        assert!(!state.dark(ThemeMode::System));
        assert!(state.dark(ThemeMode::Dark));
        state.system_changed(Some(true));
        assert!(state.system_available);
        assert!(state.dark(ThemeMode::System));
    }
}
