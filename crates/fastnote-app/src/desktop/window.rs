use std::rc::Rc;
use std::time::Duration;

use fastnote_platform::{capture_frameless_geometry, capture_geometry, NativeWindowHandle};
use slint::winit_030::{
    winit::{
        event::WindowEvent,
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    },
    EventResult, WinitWindowAccessor,
};
use slint::{ComponentHandle, Timer, TimerMode};

use super::runtime::Runtime;

pub(super) fn native_handle(window: &Window) -> Option<NativeWindowHandle> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(NativeWindowHandle(handle.hwnd.get())),
        _ => None,
    }
}

impl Runtime {
    pub(super) fn connect(self: &Rc<Self>) {
        self.shell.component().on_desktop_wakeup({
            let weak = Rc::downgrade(self);
            move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.wakeup();
                }
            }
        });
        let weak_window = self.shell.component().as_weak();
        self.shell.component().window().on_close_requested(move || {
            // 等异步保存完成再 hide/quit，不能在 winit 借用回调里直接销毁原生窗口。
            let _ = weak_window
                .upgrade_in_event_loop(|window| window.invoke_route_action("desktop:close".into()));
            slint::CloseRequestResponse::KeepWindowShown
        });
        self.shell.component().window().on_winit_window_event({
            let weak = Rc::downgrade(self);
            move |_, event| {
                if let Some(runtime) = weak.upgrade() {
                    if let WindowEvent::Focused(active) = event {
                        runtime.shell.component().set_window_active(*active);
                    }
                    if matches!(
                        event,
                        WindowEvent::ThemeChanged(_) | WindowEvent::Focused(true)
                    ) {
                        let weak = Rc::downgrade(&runtime);
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Some(runtime) = weak.upgrade() {
                                runtime.refresh_system_theme();
                            }
                        });
                    }
                    if matches!(
                        event,
                        WindowEvent::Moved(_)
                            | WindowEvent::Resized(_)
                            | WindowEvent::ScaleFactorChanged { .. }
                    ) {
                        runtime.schedule_geometry();
                        let weak = Rc::downgrade(&runtime);
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Some(runtime) = weak.upgrade() {
                                runtime.sync_minimized_timers();
                                // 窗口创建/重建后的首个 Resized，补齐任务栏大图标。
                                runtime.apply_main_icon();
                                runtime.apply_main_caption();
                                runtime.sync_window_state();
                            }
                        });
                    }
                }
                EventResult::Propagate
            }
        });
        self.ball.on_pointer_phase({
            let weak = Rc::downgrade(self);
            move |phase| {
                if let Some(runtime) = weak.upgrade() {
                    runtime.ball_pointer(phase);
                }
            }
        });
        self.ball
            .window()
            .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
        self.ball.window().on_winit_window_event({
            let weak = Rc::downgrade(self);
            move |_, event| {
                if matches!(event, WindowEvent::ThemeChanged(_)) {
                    let weak = weak.clone();
                    Timer::single_shot(Duration::ZERO, move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.refresh_system_theme();
                        }
                    });
                }
                if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
                    let weak = weak.clone();
                    Timer::single_shot(Duration::ZERO, move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.ball.window().with_winit_window(|window| {
                                if let Some(handle) = native_handle(window) {
                                    if fastnote_platform::configure_ball_window(handle).is_err() {
                                        tracing::warn!(
                                            "floating ball role restore failed after DPI change"
                                        );
                                    }
                                }
                            });
                        }
                    });
                }
                EventResult::Propagate
            }
        });
        self.ball.on_menu_requested({
            let weak = Rc::downgrade(self);
            move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.show_menu();
                }
            }
        });
        self.menu.on_action({
            let weak = Rc::downgrade(self);
            move |action| {
                let weak = weak.clone();
                let action = action.to_string();
                // 菜单自己的输入事件结束后再隐藏/切换窗口，不保留当前 winit 窗口。
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.menu_action(action);
                    }
                });
            }
        });
        self.menu.on_dismissed({
            let weak = Rc::downgrade(self);
            move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.hide_menu();
                }
            }
        });
        self.menu.window().on_winit_window_event({
            let weak = Rc::downgrade(self);
            move |_, event| {
                if matches!(event, WindowEvent::ThemeChanged(_)) {
                    let weak = weak.clone();
                    Timer::single_shot(Duration::ZERO, move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.refresh_system_theme();
                        }
                    });
                }
                if matches!(
                    event,
                    WindowEvent::Focused(false) | WindowEvent::CloseRequested
                ) {
                    let weak = weak.clone();
                    Timer::single_shot(Duration::ZERO, move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.hide_menu();
                        }
                    });
                }
                EventResult::Propagate
            }
        });
    }

    pub(super) fn sync_window_state(&self) {
        self.shell.component().window().with_winit_window(|window| {
            self.shell
                .component()
                .set_window_maximized(window.is_maximized());
            self.shell.component().set_window_active(window.has_focus());
        });
    }

    pub(super) fn minimize_window(self: &Rc<Self>) {
        if !self.main_visible() || self.finished.get() || self.closing.get() {
            return;
        }
        if self.entry.get() == Some(fastnote_core::EntryMode::Ball) {
            self.request_minimize();
        } else {
            self.shell.component().window().set_minimized(true);
            self.shell.pause_timers();
        }
    }

    pub(super) fn maximize_window(&self) {
        if !self.main_visible() || self.finished.get() || self.closing.get() {
            return;
        }
        let window = self.shell.component().window();
        if let Some(maximized) = window.with_winit_window(|window| window.is_maximized()) {
            window.set_maximized(!maximized);
            self.shell.component().set_window_maximized(!maximized);
        }
    }

    fn schedule_geometry(self: &Rc<Self>) {
        if self.closing.get() || self.finished.get() || !self.main_visible() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.geometry_timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(500),
            move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.persist_geometry();
                }
            },
        );
    }

    pub(super) fn capture_main(&self) -> Result<(), ()> {
        if !self.main_visible() {
            return Ok(());
        }
        let result = self
            .shell
            .component()
            .window()
            .with_winit_window(|window| {
                native_handle(window).ok_or(()).and_then(|handle| {
                    let result = if window.is_decorated() {
                        capture_geometry(handle)
                    } else {
                        capture_frameless_geometry(handle)
                    };
                    result.map_err(|error| {
                        tracing::warn!(
                            phase = "main_geometry",
                            error_kind = error.diagnostic_kind(),
                            native_code = error.native_code()
                        );
                    })
                })
            })
            .ok_or(())??;
        if !result.is_valid() {
            return Err(());
        }
        self.geometry.set(Some(result));
        Ok(())
    }
}
