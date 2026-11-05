use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, RenderingState, Timer};

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
                let generation = self.ui_epoch.get().session().unwrap_or(0);
                let started = self.started;
                let mut first_frame = true;
                if let Err(error) =
                    window
                        .window()
                        .set_rendering_notifier(move |state, _| match state {
                            RenderingState::RenderingSetup => {
                                first_frame = true;
                                tracing::info!(
                                    phase = "renderer_setup",
                                    session_generation = generation
                                );
                            }
                            RenderingState::RenderingTeardown => {
                                tracing::info!(
                                    phase = "renderer_teardown",
                                    session_generation = generation
                                );
                            }
                            RenderingState::AfterRendering if first_frame => {
                                first_frame = false;
                                tracing::info!(
                                    phase = "main_first_frame",
                                    session_generation = generation,
                                    elapsed_ms = started.elapsed().as_millis() as u64
                                );
                            }
                            _ => {}
                        })
                {
                    tracing::warn!(phase = "renderer_observer", error_kind = ?error);
                }
                ui.shell.on_dialog_resolved({
                    let weak = self.ui_target();
                    move |command, outcome| {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.observe_dialog_resolution(command, outcome);
                        }
                    }
                });
                window.on_desktop_wakeup({
                    let weak = self.ui_target();
                    move || {
                        if let Some(runtime) = weak.upgrade() {
                            runtime.wakeup();
                        }
                    }
                });
                window.window().on_close_requested({
                    let weak = self.ui_target();
                    move || {
                        let Some(runtime) = weak.upgrade() else {
                            return slint::CloseRequestResponse::KeepWindowShown;
                        };
                        let weak = runtime.page_target();
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
                    let weak = self.ui_target();
                    move |_, event| {
                        if let Some(runtime) = weak.upgrade() {
                            if runtime.main_activation_pending() {
                                runtime.defer_main_activation();
                            }
                            if let WindowEvent::Focused(active) = event {
                                let _ = runtime
                                    .with_ui(|ui| ui.shell.component().set_window_active(*active));
                                if !active {
                                    let weak = runtime.page_target();
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
                                let weak = runtime.ui_target();
                                Timer::single_shot(Duration::ZERO, move || {
                                    if let Some(runtime) = weak.upgrade() {
                                        runtime.refresh_system_theme();
                                        runtime.sync_minimized_timers();
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
                                let weak = runtime.ui_target();
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
