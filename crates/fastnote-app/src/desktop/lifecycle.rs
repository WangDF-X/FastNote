mod recovery;

use std::rc::Rc;

use fastnote_core::{restore_main, DesktopSetting, DesktopSettings, EntryMode};
use fastnote_platform::{monitors, WindowsHotkey};
use fastnote_store::StoreError;
use fastnote_ui::{storage_message, ToastKind};
use slint::ComponentHandle;

use super::presentation::Presentation;
use super::runtime::{Completion, Runtime};

impl Runtime {
    pub(super) fn loaded(self: &Rc<Self>, result: Result<DesktopSettings, StoreError>) {
        if self.finished.get() {
            return;
        }
        self.set_busy(false);
        match result {
            Ok(settings) => {
                self.task_preferences_error.borrow_mut().take();
                self.ready.set(true);
                self.set_desktop_state(1);
                self.geometry.set(settings.geometry);
                *self.ball_position.borrow_mut() = settings.ball_position.clone();
                let mode = settings.entry_mode;
                *self.settings.borrow_mut() = settings;
                self.sync_settings();
                match monitors() {
                    Ok(monitors) => {
                        let restore = restore_main(self.geometry.get(), &monitors);
                        if restore.is_some_and(|restore| restore.corrected) {
                            self.notice(ToastKind::Info, "窗口位置已调整到当前可见屏幕。");
                        }
                        self.restore.set(restore);
                    }
                    Err(_) => self.notice(
                        ToastKind::Warning,
                        "无法读取屏幕信息，将使用系统默认窗口位置。",
                    ),
                }
                let first = !self.initialized.replace(true);
                self.start_kernel_tasks();
                if !first || !self.hidden_start {
                    // 先确定主窗呈现，再配置入口；手动启动不能先闪现球。
                    self.show_main();
                }
                self.register_hotkey();
                if self.change_entry(mode).is_err() {
                    let fallback = if mode == EntryMode::Ball {
                        EntryMode::Tray
                    } else {
                        EntryMode::Ball
                    };
                    if self.change_entry(fallback).is_ok() {
                        self.notice(
                            ToastKind::Warning,
                            "所选入口暂不可用，已临时使用另一种入口；原配置未改动。",
                        );
                    } else {
                        self.entry.set(None);
                        self.notice(
                            ToastKind::Error,
                            "桌面入口不可用，主窗口将保持显示，关闭时退出应用。",
                        );
                    }
                }
                if self.entry.get().is_none() {
                    self.show_recovery_main("startup_entry_recovery");
                    if self.finished.get() {
                        return;
                    }
                }
                if first {
                    tracing::info!(
                        phase = "desktop_initialized",
                        elapsed_ms = self.started.elapsed().as_millis() as u64
                    );
                }
            }
            Err(error) => {
                self.ready.set(false);
                let message = if matches!(error, StoreError::NewerSchema { .. }) {
                    "数据由更高版本创建，请升级应用。"
                } else {
                    storage_message(error.code())
                };
                self.set_storage_error(message, error.log_id());
                // 不能隐藏启动错误；不创建/覆盖另一份库，不显示伪可用的设置。
                self.show_recovery_main("storage_error_recovery");
            }
        }
    }

    fn register_hotkey(&self) {
        self.hotkey.borrow_mut().take();
        let settings = self.settings.borrow();
        let status = if !settings.hotkeys_enabled {
            "已禁用".to_owned()
        } else if settings.show_hotkey.is_empty() {
            "未设置（未注册）".to_owned()
        } else {
            match WindowsHotkey::register(&settings.show_hotkey) {
                Ok(hotkey) => {
                    *self.hotkey.borrow_mut() = Some(hotkey);
                    format!("{} · 已注册", settings.show_hotkey)
                }
                Err(error) => {
                    tracing::warn!(
                        error_kind = error.diagnostic_kind(),
                        native_code = error.native_code(),
                        "show hotkey registration failed"
                    );
                    self.notice(
                        ToastKind::Warning,
                        "全局唤起快捷键注册失败，可能已被占用；仍可通过悬浮球或托盘打开。",
                    );
                    format!("{} · 注册失败，可通过入口打开", settings.show_hotkey)
                }
            }
        };
        drop(settings);
        self.set_hotkey_status(status);
    }

    pub(super) fn saved(self: &Rc<Self>, id: u64, result: Result<(), StoreError>) {
        let Some(pending) = self.pending.borrow_mut().remove(&id) else {
            return;
        };
        if let Err(error) = result {
            match pending.completion {
                Completion::Background => {
                    if !self.save_failed.replace(true) {
                        self.notice(ToastKind::Error, storage_message(error.code()));
                    }
                }
                Completion::Close { exit } => {
                    self.closing.set(false);
                    self.set_busy(false);
                    self.close_failed(exit, storage_message(error.code()), error.log_id());
                }
                Completion::Preference { .. } | Completion::Rollback => {
                    self.settle_theme(&pending.changes);
                    self.set_busy(false);
                    self.preference_repair.set(true);
                    self.notice(ToastKind::Error, storage_message(error.code()));
                    self.sync_settings();
                }
                Completion::TaskPreference { .. } => self.task_preference_failed(&error),
            }
            return;
        }
        self.save_failed.set(false);
        if let Completion::Preference { previous } = &pending.completion {
            if let Some(DesktopSetting::EntryMode(mode)) = pending.changes.first() {
                if self.change_entry(*mode).is_err() {
                    if self.entry.get().is_none() {
                        self.show_recovery_main("preference_entry_recovery");
                        if self.finished.get() {
                            return;
                        }
                        self.notice(ToastKind::Error, "桌面入口不可用，关闭主窗口时将退出应用。");
                    }
                    self.notice(ToastKind::Error, "入口切换失败，正在恢复原配置。");
                    if !self.queue(vec![previous.clone()], Completion::Rollback, false) {
                        self.preference_repair.set(true);
                        self.set_busy(false);
                    }
                    return;
                }
            }
        }
        for change in &pending.changes {
            self.settings.borrow_mut().apply(change);
        }
        self.settle_theme(&pending.changes);
        self.sync_settings();
        match pending.completion {
            Completion::Background => {}
            Completion::Preference { .. } | Completion::Rollback => {
                self.preference_repair.set(false);
                self.set_busy(false);
            }
            Completion::TaskPreference { reveal_unscheduled } => {
                self.task_preference_committed(reveal_unscheduled);
            }
            Completion::Close { exit } => {
                self.complete_close(exit, &pending.changes);
            }
        }
    }

    pub(super) fn finish_exit(&self) {
        if self.finished.replace(true) {
            return;
        }
        self.cleanup();
        if slint::quit_event_loop().is_err() {
            tracing::error!("failed to quit UI event loop");
        }
    }

    pub(super) fn cleanup(&self) {
        super::wakeup::unregister(self);
        self.pending_effects.borrow_mut().clear();
        self.theme_monitor.borrow_mut().take();
        self.geometry_timer.stop();
        self.task_day_timer.stop();
        let _ = self.with_ui(|ui| {
            ui.task_save_timer.stop();
            ui.task_feedback_timer.stop();
            ui.shell.pause_timers();
        });
        self.hide_menu();
        if self.ball.hide().is_err() {
            tracing::warn!("floating ball hide failed during exit");
        }
        self.tray.borrow_mut().take();
        self.hotkey.borrow_mut().take();
        self.entry.set(None);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_entry_available(false);
            if ui.shell.hide().is_err() {
                tracing::warn!("main window hide failed during exit");
            }
        });
        self.presentation.set(Presentation::Hidden);
    }
}
