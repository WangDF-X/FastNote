mod dispatch;

use std::rc::Rc;

use fastnote_core::{CloseBehavior, DesktopSetting, EntryMode, ThemeMode};
use fastnote_platform::{EntryAction, NativeEvent};
use fastnote_ui::{ShellUpdate, ToastKind};

use super::effects::UiEffect;
use super::runtime::{Completion, Runtime};
use crate::routing::parse_action;
use crate::tasks::Transition;

use dispatch::{dispatch_or_handle, should_refresh_today};

impl Runtime {
    pub(super) fn handle_action(self: &Rc<Self>, action: &str) -> ShellUpdate {
        if self.finished.get() {
            return self.unchanged_shell();
        }
        if self.handle_task_preferences_action(action) || self.handle_task_action(action) {
            return ShellUpdate {
                view: self.controller.borrow().shell_view(),
                route_changed: false,
                issue: None,
            };
        }
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
                    self.set_desktop_state(0);
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
            _ => {
                let navigation = action.starts_with("navigate:") || action == "back";
                if navigation
                    && (self.dialog_blocked()
                        || (parse_action(action).is_ok()
                            && self.defer_task_transition(Transition::Navigate(action.to_owned()))))
                {
                    return ShellUpdate {
                        view: self.controller.borrow().shell_view(),
                        route_changed: false,
                        issue: None,
                    };
                }
                let update = self.controller.borrow_mut().handle_action(action);
                if update.route_changed {
                    self.today_dirty.set(true);
                    self.close_task_editor();
                    if should_refresh_today(&update, self.with_ui(|_| ()).is_some()) {
                        self.request_today();
                    }
                }
                return update;
            }
        }
        ShellUpdate {
            view: self.controller.borrow().shell_view(),
            route_changed: false,
            issue: None,
        }
    }

    fn preference(self: &Rc<Self>, change: DesktopSetting) {
        if !can_change_desktop(
            self.ready.get(),
            self.finished.get(),
            self.closing.get(),
            self.busy.get(),
            self.dialog_blocked(),
        ) {
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
                self.dispatch_action(format!("navigate:{page}"));
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
        self.dispatch_action(action);
        if navigate {
            self.show_main();
        }
    }

    /// 桌面入口不依赖外壳存在；无会话时立即确认路由，仅排队展示，不重放业务。
    pub(super) fn dispatch_action(self: &Rc<Self>, action: String) {
        if self.finished.get() {
            return;
        }
        let update = dispatch_or_handle(
            &action,
            |action| {
                self.with_ui(|ui| ui.shell.dispatch(action.to_owned()))
                    .is_some()
            },
            |action| self.handle_action(action),
        );
        if let Some(update) = update {
            if update.route_changed {
                self.pending_effects.borrow_mut().discard_route_toasts();
                self.emit_ui(UiEffect::Navigate(update.view.route_debug));
            }
            if let Some(issue) = update.issue {
                self.notice(ToastKind::Error, issue.message());
            }
        }
    }

    fn unchanged_shell(&self) -> ShellUpdate {
        ShellUpdate {
            view: self.controller.borrow().shell_view(),
            route_changed: false,
            issue: None,
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

fn can_change_desktop(
    ready: bool,
    finished: bool,
    closing: bool,
    busy: bool,
    dialog: bool,
) -> bool {
    ready && !finished && !closing && !busy && !dialog
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

    #[test]
    fn test_desktop_preferences_do_not_require_ui_but_respect_kernel_and_modal_guards() {
        assert!(can_change_desktop(true, false, false, false, false));
        for (ready, finished, closing, busy, dialog) in [
            (false, false, false, false, false),
            (true, true, false, false, false),
            (true, false, true, false, false),
            (true, false, false, true, false),
            (true, false, false, false, true),
        ] {
            assert!(!can_change_desktop(ready, finished, closing, busy, dialog));
        }
    }
}
