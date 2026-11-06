use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Timer};

use super::activation::{ActivationStep, MainActivation};
use super::Presentation;
use crate::desktop::{runtime::Runtime, window::native_handle};

impl Runtime {
    pub(in crate::desktop) fn defer_main_activation(self: &Rc<Self>) {
        let target = self.ui_target();
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = target.upgrade() {
                runtime.advance_main_activation();
            }
        });
    }

    /// 由实际原生事件/显示请求推进，不轮询、不持续抢焦点；完成一次后回到 Idle。
    pub(in crate::desktop) fn advance_main_activation(self: &Rc<Self>) {
        if self.finished.get() || self.closing.get() {
            return;
        }
        let step = self
            .with_ui(|ui| {
                let window = ui.shell.component().window();
                let native = window.with_winit_window(|native| {
                    (
                        native.is_minimized() == Some(true),
                        native.is_visible() != Some(false),
                    )
                });
                let mut state = ui.main_activation.get();
                let (minimized, visible) = native.unwrap_or((false, false));
                let step = state.advance(
                    self.presentation.get() == Presentation::Main,
                    native.is_some(),
                    minimized,
                    visible,
                );
                ui.main_activation.set(state);
                step
            })
            .unwrap_or(ActivationStep::Wait);
        match step {
            ActivationStep::Wait => {}
            ActivationStep::Configure { restore_geometry } => {
                let _ = self.with_ui(|ui| {
                    let window = ui.shell.component().window();
                    window.set_minimized(false);
                    if restore_geometry {
                        if let Some(geometry) = self
                            .geometry
                            .get()
                            .or_else(|| self.restore.get().map(|restore| restore.geometry))
                        {
                            window.set_size(slint::LogicalSize::new(
                                geometry.w as f32,
                                geometry.h as f32,
                            ));
                            window.set_maximized(geometry.maximized);
                        }
                    }
                    window.with_winit_window(|native| {
                        self.main_native.set(native_handle(native));
                        native.set_minimized(false);
                    });
                });
                self.apply_main_caption();
                self.apply_main_icon();
                self.sync_window_state();
                // set_minimized/maximized 的原生事件必须处理完再申请激活。
                self.defer_main_activation();
            }
            ActivationStep::Restore => {
                let _ = self.with_ui(|ui| {
                    ui.shell
                        .component()
                        .window()
                        .with_winit_window(|native| native.set_minimized(false))
                });
                // 等实际 Resized/Focused 事件，不用零延迟循环不断恢复/抢焦点。
            }
            ActivationStep::Activate => {
                let _ = self.with_ui(|ui| {
                    ui.shell
                        .component()
                        .window()
                        .with_winit_window(|native| native.focus_window());
                    ui.shell.resume_timers();
                    ui.content_active.set(true);
                });
                self.start_page_tasks();
                tracing::info!(
                    phase = "main_native_activated",
                    session_generation = self.ui_epoch.get().session().unwrap_or(0)
                );
            }
        }
    }

    pub(in crate::desktop) fn main_activation_pending(&self) -> bool {
        self.with_ui(|ui| ui.main_activation.get() != MainActivation::Idle)
            .unwrap_or(false)
    }
}
