use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Timer};

use crate::desktop::runtime::Runtime;

impl Runtime {
    /// 主窗及任务回调只随既有会话绑定一次；无会话不绑定，也不创建任何窗口。
    pub(in crate::desktop) fn connect_ui(self: &Rc<Self>) {
        let connected = self
            .with_ui(|ui| {
                if ui.callbacks_connected.replace(true) {
                    return false;
                }
                let window = ui.shell.component();
                window.on_desktop_wakeup({
                    let weak = Rc::downgrade(self);
                    move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.wakeup();
                        }
                    }
                });
                window.window().on_close_requested({
                    let weak = Rc::downgrade(self);
                    move || {
                        let weak = weak.clone();
                        // winit 输入事件结束后才保存/隐藏，闭包不保留组件或原生窗口。
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Some(runtime) = weak.upgrade() {
                                runtime.request_close(false);
                            }
                        });
                        slint::CloseRequestResponse::KeepWindowShown
                    }
                });
                window.window().on_winit_window_event({
                    let weak = Rc::downgrade(self);
                    move |_, event| {
                        if let Some(runtime) = weak.upgrade() {
                            if let WindowEvent::Focused(active) = event {
                                let _ = runtime
                                    .with_ui(|ui| ui.shell.component().set_window_active(*active));
                                if !active {
                                    let weak = Rc::downgrade(&runtime);
                                    Timer::single_shot(Duration::ZERO, move || {
                                        if let Some(runtime) = weak.upgrade() {
                                            runtime.flush_task_editor();
                                        }
                                    });
                                }
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
                true
            })
            .unwrap_or(false);
        if connected {
            self.connect_tasks();
        }
    }
}
