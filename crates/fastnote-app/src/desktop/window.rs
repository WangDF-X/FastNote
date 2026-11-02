use std::rc::Rc;
use std::time::Duration;

use fastnote_platform::{capture_geometry, NativeWindowHandle};
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
                    capture_geometry(handle).map_err(|error| {
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
