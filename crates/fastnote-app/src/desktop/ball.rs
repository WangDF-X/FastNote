use std::rc::Rc;

use fastnote_core::{restore_ball, snap_ball, DesktopSetting, EntryMode};
use fastnote_platform::{
    configure_ball_window, configure_menu_window, cursor_position, monitor_at_point, monitors,
    WindowsTray,
};
use fastnote_ui::ToastKind;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use super::gesture::BallDrag;
use super::presentation::{entry_view, Presentation};
use super::runtime::{Completion, Runtime};
use super::window::native_handle;

const BALL_WINDOW: f64 = 48.0;

impl Runtime {
    pub(super) fn change_entry(&self, mode: EntryMode) -> Result<(), ()> {
        if self.entry.get() == Some(mode) {
            return Ok(());
        }
        let previous = self.entry.get();
        let result = match mode {
            EntryMode::Tray => {
                if previous == Some(EntryMode::Ball) {
                    let target = entry_view(self.presentation.get(), mode);
                    self.switch_presentation(target).map_err(|_| ())?;
                }
                match WindowsTray::new() {
                    Ok(tray) => {
                        *self.tray.borrow_mut() = Some(tray);
                        Ok(())
                    }
                    Err(error) => {
                        tracing::warn!(
                            error_kind = error.diagnostic_kind(),
                            native_code = error.native_code(),
                            "tray creation failed"
                        );
                        Err(())
                    }
                }
            }
            EntryMode::Ball => {
                if let Some(tray) = self.tray.borrow().as_ref() {
                    tray.set_visible(false).map_err(|_| ())?;
                }
                let target = entry_view(self.presentation.get(), mode);
                self.switch_presentation(target).map_err(|_| ())
            }
        };
        if result.is_ok() {
            self.entry.set(Some(mode));
            let _ = self.with_ui(|ui| ui.shell.component().set_entry_available(true));
            if mode == EntryMode::Ball {
                self.tray.borrow_mut().take();
            }
            return Ok(());
        }
        // 保留旧句柄直到目标入口成功；失败不修改内存设置，仓储提交另行回滚。
        let restored = match previous {
            Some(EntryMode::Ball) => self
                .switch_presentation(entry_view(self.presentation.get(), EntryMode::Ball))
                .is_ok(),
            Some(EntryMode::Tray) => {
                self.switch_presentation(entry_view(self.presentation.get(), EntryMode::Tray))
                    .is_ok()
                    && self
                        .tray
                        .borrow()
                        .as_ref()
                        .is_some_and(|tray| tray.set_visible(true).is_ok())
            }
            None => false,
        };
        if !restored {
            self.entry.set(None);
        }
        let _ = self.with_ui(|ui| ui.shell.component().set_entry_available(restored));
        Err(())
    }

    pub(super) fn show_ball_window(&self) -> Result<(), ()> {
        self.refresh_system_theme();
        let monitors = monitors().map_err(|_| ())?;
        let saved = self.ball_position.borrow();
        let (position, _) = restore_ball(saved.as_ref(), &monitors, BALL_WINDOW).ok_or(())?;
        let monitor = monitors
            .iter()
            .find(|monitor| monitor.name == position.monitor)
            .ok_or(())?;
        let (x, y) = monitor.to_physical(position.x, position.y);
        self.ball
            .window()
            .set_position(slint::PhysicalPosition::new(
                x.round() as i32,
                y.round() as i32,
            ));
        self.ball.show().map_err(|_| ())?;
        let configured = self
            .ball
            .window()
            .with_winit_window(|window| {
                native_handle(window)
                    .ok_or(())
                    .and_then(|handle| configure_ball_window(handle).map_err(|_| ()))
            })
            .unwrap_or(Err(()));
        configured?;
        drop(saved);
        *self.ball_position.borrow_mut() = Some(position);
        Ok(())
    }

    pub(super) fn ball_pointer(self: &Rc<Self>, phase: i32) {
        if self.presentation.get() != Presentation::Ball
            || self.finished.get()
            || self.closing.get()
        {
            return;
        }
        let Ok(pointer) = cursor_position() else {
            return;
        };
        match phase {
            0 => {
                self.hide_menu();
                if let Some(Some((position, scale))) =
                    self.ball.window().with_winit_window(|window| {
                        window
                            .outer_position()
                            .ok()
                            .map(|position| ((position.x, position.y), window.scale_factor()))
                    })
                {
                    *self.drag.borrow_mut() = Some(BallDrag::new(pointer, position, scale));
                }
            }
            1 => self.move_ball(pointer),
            2 | 3 => {
                self.move_ball(pointer);
                let moved = self
                    .drag
                    .borrow_mut()
                    .take()
                    .is_some_and(|drag| drag.moved());
                if moved {
                    if self.capture_ball_position_at(Some(pointer)).is_err() {
                        self.notice(ToastKind::Error, "入口位置暂未保存，请重新拖动后重试。");
                        return;
                    }
                    if let Some(position) = self.ball_position.borrow().clone() {
                        self.queue(
                            vec![DesktopSetting::BallPosition(Some(position))],
                            Completion::Background,
                            false,
                        );
                    }
                } else if phase == 2 {
                    self.defer_show_main();
                }
            }
            _ => {}
        }
    }

    fn move_ball(&self, pointer: (i32, i32)) {
        if let Some(position) = self
            .drag
            .borrow_mut()
            .as_mut()
            .and_then(|drag| drag.update(pointer))
        {
            self.ball
                .window()
                .set_position(slint::PhysicalPosition::new(position.0, position.1));
        }
    }

    pub(super) fn capture_ball_position(&self) -> Result<(), ()> {
        self.capture_ball_position_at(None)
    }

    fn capture_ball_position_at(&self, pointer: Option<(i32, i32)>) -> Result<(), ()> {
        if self.presentation.get() != Presentation::Ball {
            return Ok(());
        }
        let position = self
            .ball
            .window()
            .with_winit_window(|window| window.outer_position().ok())
            .flatten()
            .ok_or(())?;
        let size = self.ball.window().size();
        let point = pointer.unwrap_or((
            position.x.saturating_add((size.width / 2) as i32),
            position.y.saturating_add((size.height / 2) as i32),
        ));
        let monitor = monitor_at_point(point.0, point.1).map_err(|_| ())?;
        let (x, y) = monitor.to_logical(f64::from(position.x), f64::from(position.y));
        let saved = snap_ball(x, y, BALL_WINDOW, &monitor).ok_or(())?;
        let (x, y) = monitor.to_physical(saved.x, saved.y);
        self.ball
            .window()
            .set_position(slint::PhysicalPosition::new(
                x.round() as i32,
                y.round() as i32,
            ));
        *self.ball_position.borrow_mut() = Some(saved);
        Ok(())
    }

    pub(super) fn show_menu(&self) {
        if self.presentation.get() != Presentation::Ball || self.finished.get() {
            return;
        }
        self.refresh_system_theme();
        let position = self.ball.window().position();
        let size = self.ball.window().size();
        let Ok(monitor) = monitor_at_point(
            position.x.saturating_add((size.width / 2) as i32),
            position.y.saturating_add((size.height / 2) as i32),
        ) else {
            return;
        };
        let (x, y) = monitor.to_logical(f64::from(position.x), f64::from(position.y));
        let (width, height) = (232.0, 380.0);
        let work = monitor.work;
        let menu_x = if x + BALL_WINDOW + width <= work.x + work.w {
            x + BALL_WINDOW
        } else {
            x - width
        };
        let menu_y = if y + height <= work.y + work.h {
            y
        } else {
            y + BALL_WINDOW - height
        };
        let menu_x = menu_x.clamp(work.x, (work.x + work.w - width).max(work.x));
        let menu_y = menu_y.clamp(work.y, (work.y + work.h - height).max(work.y));
        let (x, y) = monitor.to_physical(menu_x, menu_y);
        self.menu
            .window()
            .set_position(slint::PhysicalPosition::new(
                x.round() as i32,
                y.round() as i32,
            ));
        self.menu_open.set(true);
        if self.menu.show().is_err() {
            self.menu_open.set(false);
            self.notice(
                ToastKind::Error,
                "入口菜单无法打开，请点击悬浮球进入主窗口。",
            );
            return;
        }
        self.menu.invoke_focus_menu();
        let configured = self
            .menu
            .window()
            .with_winit_window(|window| {
                native_handle(window)
                    .ok_or(())
                    .and_then(|handle| configure_menu_window(handle).map_err(|_| ()))
            })
            .unwrap_or(Err(()));
        if configured.is_err() {
            self.hide_menu();
            self.notice(
                ToastKind::Error,
                "无法创建入口菜单，请点击悬浮球进入主窗口。",
            );
            return;
        }
        self.menu
            .window()
            .with_winit_window(|window| window.focus_window());
    }

    pub(super) fn hide_menu(&self) {
        if self.menu_open.replace(false) && self.menu.hide().is_err() {
            tracing::warn!("ball menu hide failed");
        }
    }
}
