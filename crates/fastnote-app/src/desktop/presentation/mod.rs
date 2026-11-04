mod state;

use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{DesktopSetting, EntryMode};
use fastnote_ui::ToastKind;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Timer};

use super::effects::{FocusTarget, UiEffect};
use super::runtime::{Completion, Runtime};
use super::window::native_handle;
use state::{switch, Surfaces};

pub(super) use state::{entry_view, minimize_to_ball, Presentation, SwitchError};
use state::{minimize_action, MinimizeAction};

impl Runtime {
    pub(super) fn main_visible(&self) -> bool {
        self.presentation.get() == Presentation::Main && self.with_ui(|_| ()).is_some()
    }

    pub(super) fn switch_presentation(&self, target: Presentation) -> Result<(), SwitchError> {
        if target == Presentation::Main && self.with_ui(|_| ()).is_none() {
            return Err(SwitchError::ShowTarget);
        }
        let capture_failed = self.presentation.get() == Presentation::Ball
            && target != Presentation::Ball
            && self.capture_ball_position().is_err();
        self.hide_menu();
        self.drag.borrow_mut().take();
        let result = switch(self, &self.presentation, target).inspect_err(|error| {
            tracing::error!(phase = "presentation_switch", error_kind = ?error);
        });
        if result.is_ok() {
            if capture_failed {
                self.notice(ToastKind::Warning, "未能读取入口位置，将保留上次已知位置。");
            }
            self.persist_ball_position();
        }
        result
    }

    pub(super) fn hide_to_entry(&self) -> Result<(), ()> {
        let target = match self.entry.get() {
            Some(EntryMode::Ball) => Presentation::Ball,
            Some(EntryMode::Tray) => Presentation::Hidden,
            None => return Err(()),
        };
        self.switch_presentation(target).map_err(|_| ())?;
        Ok(())
    }

    /// 当前球输入事件结束后再切换，避免事件分发仍持有原生窗口引用而短暂双显。
    pub(super) fn defer_show_main(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                runtime.show_main();
            }
        });
    }

    pub(super) fn show_main(self: &Rc<Self>) {
        if self.finished.get() || self.closing.get() {
            return;
        }
        if self.ensure_ui().is_err() {
            self.notice(ToastKind::Error, "无法创建主窗口，请通过入口或快捷键重试。");
            return;
        }
        self.hide_menu();
        let result = if self.main_visible() {
            self.show_main_window().map_err(|_| SwitchError::ShowTarget)
        } else {
            self.switch_presentation(Presentation::Main)
        };
        if result.is_err() {
            self.notice(ToastKind::Error, "无法显示主窗口，请通过入口或快捷键重试。");
            return;
        }
        self.emit_ui(UiEffect::Focus(FocusTarget::Main));
        // 首次创建后的列表读取只为当前今日路由；完整可见性/日期策略由 T2.3/T2.4 收敛。
        if self.today_dirty.get() && self.controller.borrow().shell_view().page_label == "今日任务"
        {
            self.request_today();
        }
        self.persist_ball_position();
        if self.ready.get() && !self.settings.borrow().first_run_done {
            self.queue(
                vec![DesktopSetting::FirstRunDone(true)],
                Completion::Background,
                false,
            );
        }
    }

    fn persist_ball_position(&self) {
        if !self.ready.get() {
            return;
        }
        let position = self.ball_position.borrow().clone();
        if position.is_some() && position != self.settings.borrow().ball_position {
            self.queue(
                vec![DesktopSetting::BallPosition(position)],
                Completion::Background,
                false,
            );
        }
    }

    pub(super) fn sync_minimized_timers(self: &Rc<Self>) {
        if !self.main_visible() || self.finished.get() {
            return;
        }
        let minimized = self
            .with_ui(|ui| {
                ui.shell
                    .component()
                    .window()
                    .with_winit_window(|window| window.is_minimized().unwrap_or(false))
            })
            .flatten();
        match minimize_action(
            self.presentation.get(),
            self.entry.get(),
            minimized,
            self.ready.get(),
            self.busy.get() || self.closing.get(),
        ) {
            MinimizeAction::Ignore => {}
            MinimizeAction::Pause => {
                let _ = self.with_ui(|ui| ui.shell.pause_timers());
            }
            MinimizeAction::Resume => {
                let _ = self.with_ui(|ui| ui.shell.resume_timers());
            }
            MinimizeAction::RestoreMain => {
                self.show_main();
                self.notice(ToastKind::Error, "设置尚未加载，暂时无法收起到悬浮球。");
            }
            MinimizeAction::Collapse => {
                let _ = self.with_ui(|ui| ui.shell.pause_timers());
                self.request_minimize();
            }
        }
    }

    fn show_main_window(&self) -> Result<(), ()> {
        self.flush_ui_effects();
        self.refresh_system_theme();
        self.with_ui(|ui| {
            let window = ui.shell.component().window();
            let recreating = !window.has_winit_window();
            let geometry = self
                .geometry
                .get()
                .or_else(|| self.restore.get().map(|restore| restore.geometry));
            if recreating {
                window.set_minimized(false);
                window.set_maximized(false);
                // 原生窗口会重建，HWND 可能复用；清空大图标、标题栏和外沿去重标记。
                self.main_icon.set(0);
                self.main_caption.set(0);
                self.main_frame.set(None);
                if let Some(geometry) = geometry {
                    window.set_size(slint::LogicalSize::new(
                        geometry.w as f32,
                        geometry.h as f32,
                    ));
                    // 隐藏会释放原生窗口；恢复时重新枚举屏幕，不沿用旧物理坐标。
                    if let Ok(monitors) = fastnote_platform::monitors() {
                        if let Some(restore) =
                            fastnote_core::restore_main(Some(geometry), &monitors)
                        {
                            window.set_position(slint::PhysicalPosition::new(
                                restore.physical_x.round() as i32,
                                restore.physical_y.round() as i32,
                            ));
                            self.geometry.set(Some(restore.geometry));
                        }
                    }
                }
            }
            ui.shell.show().map_err(|_| ())?;
            self.apply_main_caption();
            self.sync_theme();
            window.set_minimized(false);
            if recreating {
                if let Some(geometry) = self.geometry.get().or(geometry) {
                    window.set_size(slint::LogicalSize::new(
                        geometry.w as f32,
                        geometry.h as f32,
                    ));
                    window.set_maximized(geometry.maximized);
                }
            }
            Ok(())
        })
        .unwrap_or(Err(()))
    }

    /// 补齐任务栏和 Alt+Tab 使用的大图标（ICON_BIG）。
    ///
    /// winit 的窗口图标只设置 ICON_SMALL（标题栏）；Slint 的图标在窗口创建前写入
    /// 属性，创建时不会再设置任务栏图标。这里在窗口真正创建后（首次 `Resized`）
    /// 按 HWND 去重设置一次，隐藏后重建会再次触发。
    pub(super) fn apply_main_icon(&self) {
        let _ = self.with_ui(|ui| {
            ui.shell
                .component()
                .window()
                .with_winit_window(|winit_window| {
                    let Some(handle) = native_handle(winit_window) else {
                        return;
                    };
                    if self.main_icon.get() == handle.0 {
                        return;
                    }
                    match fastnote_platform::set_window_big_icon(handle) {
                        Ok(()) => self.main_icon.set(handle.0),
                        Err(error) => tracing::warn!(
                            phase = "window_icon",
                            code = error.native_code(),
                            "failed to set the window big icon"
                        ),
                    }
                });
        });
    }
}

impl Surfaces for Runtime {
    fn hide(&self, view: Presentation) -> Result<(), ()> {
        match view {
            Presentation::Hidden => Ok(()),
            Presentation::Main => self
                .with_ui(|ui| ui.shell.hide().map_err(|_| ()))
                .unwrap_or(Ok(())),
            Presentation::Ball => self.ball.hide().map_err(|_| ()),
        }
    }

    fn show(&self, view: Presentation) -> Result<(), ()> {
        match view {
            Presentation::Hidden => Ok(()),
            Presentation::Main => self.show_main_window(),
            Presentation::Ball => self.show_ball_window(),
        }
    }
}
