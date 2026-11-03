use std::rc::Rc;

use fastnote_core::{CloseBehavior, DesktopSetting, EntryMode, ThemeMode};
use fastnote_platform::{EntryAction, NativeEvent};
use fastnote_ui::{ShellUpdate, ToastKind};

use super::runtime::{Completion, Runtime};

impl Runtime {
    pub(super) fn handle_action(self: &Rc<Self>, action: &str) -> ShellUpdate {
        match action {
            "desktop:show" => self.show_main(),
            "desktop:minimize" => self.minimize_window(),
            "desktop:maximize" => self.maximize_window(),
            "desktop:close" => self.request_close(false),
            "desktop:quit" => self.request_close(true),
            "desktop:force-quit" if self.force_allowed.get() => self.finish_exit(),
            "desktop:retry" => {
                if !self.busy.get() && self.worker.reload().is_ok() {
                    self.set_busy(true);
                    self.shell.component().set_desktop_state(0);
                }
            }
            "desktop:entry:ball" => self.preference(DesktopSetting::EntryMode(EntryMode::Ball)),
            "desktop:entry:tray" => self.preference(DesktopSetting::EntryMode(EntryMode::Tray)),
            "desktop:theme:cycle" => {
                let next = self.settings.borrow().theme.next_mode();
                self.preference(DesktopSetting::Theme(next));
            }
            "desktop:theme:system" => self.preference(DesktopSetting::Theme(ThemeMode::System)),
            "desktop:theme:light" => self.preference(DesktopSetting::Theme(ThemeMode::Light)),
            "desktop:theme:dark" => self.preference(DesktopSetting::Theme(ThemeMode::Dark)),
            "desktop:close-behavior:entry" => {
                self.preference(DesktopSetting::CloseBehavior(CloseBehavior::Entry))
            }
            "desktop:close-behavior:quit" => {
                self.preference(DesktopSetting::CloseBehavior(CloseBehavior::Quit))
            }
            _ => return self.controller.borrow_mut().handle_action(action),
        }
        ShellUpdate {
            view: self.controller.borrow().shell_view(),
            route_changed: false,
            issue: None,
        }
    }

    fn preference(self: &Rc<Self>, change: DesktopSetting) {
        if !self.ready.get() || self.busy.get() {
            return;
        }
        let previous = match &change {
            DesktopSetting::EntryMode(_) => {
                DesktopSetting::EntryMode(self.settings.borrow().entry_mode)
            }
            DesktopSetting::CloseBehavior(_) => {
                DesktopSetting::CloseBehavior(self.settings.borrow().close_behavior)
            }
            DesktopSetting::Theme(_) => DesktopSetting::Theme(self.settings.borrow().theme),
            _ => return,
        };
        if !should_write_preference(
            &change,
            &previous,
            self.entry.get(),
            self.preference_repair.get(),
        ) {
            return;
        }
        let theme = match change {
            DesktopSetting::Theme(mode) => Some(mode),
            _ => None,
        };
        if self.queue(vec![change], Completion::Preference { previous }, false) {
            if let Some(mode) = theme {
                self.preview_theme(mode);
            }
            self.set_busy(true);
        }
    }

    pub(super) fn native_action(self: &Rc<Self>, event: NativeEvent) {
        let action = match event {
            NativeEvent::SystemThemeChanged => {
                self.refresh_system_theme();
                None
            }
            NativeEvent::TrayShow(id) => self
                .tray
                .borrow()
                .as_ref()
                .filter(|tray| tray.matches(&id))
                .map(|_| EntryAction::Show),
            NativeEvent::Menu(id) => self
                .tray
                .borrow()
                .as_ref()
                .and_then(|tray| tray.menu_action(&id)),
            NativeEvent::HotkeyShow(id) => self
                .hotkey
                .borrow()
                .as_ref()
                .filter(|hotkey| hotkey.id() == Some(id))
                .map(|_| EntryAction::Show),
        };
        match action {
            Some(EntryAction::Show) => self.show_main(),
            Some(EntryAction::Navigate(page)) => {
                self.shell.dispatch(format!("navigate:{page}"));
                self.show_main();
            }
            Some(EntryAction::Switch(mode)) => self.preference(DesktopSetting::EntryMode(mode)),
            Some(EntryAction::Quit) => self.request_close(true),
            None => {}
        }
    }

    pub(super) fn menu_action(self: &Rc<Self>, action: String) {
        self.hide_menu();
        let navigate = action.starts_with("navigate:");
        self.shell.dispatch(action);
        if navigate {
            self.show_main();
        }
    }

    pub(super) fn persist_geometry(self: &Rc<Self>) {
        if !self.ready.get() || self.closing.get() || !self.main_visible() {
            return;
        }
        if self.capture_main().is_err() {
            if !self.save_failed.replace(true) {
                self.notice(ToastKind::Error, "无法读取窗口位置，暂未保存。");
            }
            return;
        }
        let geometry = self.geometry.get();
        if geometry != self.settings.borrow().geometry {
            self.queue(
                vec![DesktopSetting::MainGeometry(geometry)],
                Completion::Background,
                false,
            );
        }
    }
}

fn should_write_preference(
    change: &DesktopSetting,
    previous: &DesktopSetting,
    active: Option<EntryMode>,
    repair: bool,
) -> bool {
    change != previous
        || repair
        || matches!(change, DesktopSetting::EntryMode(mode) if active != Some(*mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preferred_entry_can_be_retried_after_temporary_fallback() {
        let change = DesktopSetting::EntryMode(EntryMode::Ball);
        assert!(!should_write_preference(
            &change,
            &change,
            Some(EntryMode::Ball),
            false
        ));
        assert!(should_write_preference(
            &change,
            &change,
            Some(EntryMode::Tray),
            false
        ));
        assert!(should_write_preference(&change, &change, None, false));
        assert!(should_write_preference(
            &change,
            &change,
            Some(EntryMode::Ball),
            true
        ));
    }

    #[test]
    fn test_same_close_policy_does_not_write_unless_repairing_failed_rollback() {
        let previous = DesktopSetting::CloseBehavior(CloseBehavior::Entry);
        assert!(!should_write_preference(&previous, &previous, None, false));
        assert!(should_write_preference(&previous, &previous, None, true));
        assert!(should_write_preference(
            &DesktopSetting::CloseBehavior(CloseBehavior::Quit),
            &previous,
            None,
            false
        ));
    }

    #[test]
    fn test_theme_preference_only_writes_changes_or_repairs() {
        let previous = DesktopSetting::Theme(ThemeMode::System);
        assert!(!should_write_preference(&previous, &previous, None, false));
        assert!(should_write_preference(&previous, &previous, None, true));
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            assert!(should_write_preference(
                &DesktopSetting::Theme(mode),
                &previous,
                None,
                false
            ));
        }
    }
}
