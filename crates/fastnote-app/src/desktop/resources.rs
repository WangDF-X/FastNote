mod callbacks;
mod restoration;

use std::rc::Rc;
use std::time::Duration;

use fastnote_platform::{
    process_resources, uptime_milliseconds, window_belongs_to_process, ProcessResources,
};
use slint::{ComponentHandle, Timer, TimerMode};

use super::lease::{
    LeaseAction, LeaseEvent, LeaseState, ReleaseReason, TickSource, COLD_BUDGET_BYTES,
    HOT_BUDGET_BYTES, SYSTEM_PRESSURE_PERCENT,
};
use super::runtime::Runtime;
use super::tasks::view::state::while_syncing;

struct Uptime;
impl TickSource for Uptime {
    fn milliseconds(&self) -> u64 {
        uptime_milliseconds()
    }
}

struct HiddenAt(u64);
impl TickSource for HiddenAt {
    fn milliseconds(&self) -> u64 {
        self.0
    }
}

fn pressure_reason(sample: ProcessResources) -> Option<ReleaseReason> {
    if sample.system_load_percent >= SYSTEM_PRESSURE_PERCENT {
        Some(ReleaseReason::Pressure)
    } else if sample.working_set_bytes > HOT_BUDGET_BYTES {
        Some(ReleaseReason::Budget)
    } else {
        None
    }
}

impl Runtime {
    pub(in crate::desktop) fn ui_mounted(&self) {
        self.ui_lease
            .borrow_mut()
            .apply(LeaseEvent::Mounted, &Uptime);
    }

    pub(in crate::desktop) fn exit_ui_resources(&self) {
        self.ui_lease.borrow_mut().apply(LeaseEvent::Exit, &Uptime);
        self.release_ui(ReleaseReason::Exit);
    }
    /// 草稿/未决选择才保护 UI；后台设置写入仍由内核确认，不为它续租。
    pub(in crate::desktop) fn ui_resource_protected(&self) -> bool {
        self.closing.get()
            || self.dialog_blocked()
            || self.tasks.borrow().resource_protected()
            || self.note_protected()
            || self.journal_protected()
            || self
                .with_ui(|ui| {
                    let window = ui.shell.component();
                    !window.get_task_quick_title().is_empty() || window.get_task_picker_open()
                })
                .unwrap_or(false)
    }

    /// 只在保存确认且原生隐藏成功之后调用，不从入口、主题或后台事件重新计时。
    pub(in crate::desktop) fn start_hot_lease(self: &Rc<Self>) {
        if self.main_visible() || self.with_ui(|_| ()).is_none() {
            return;
        }
        let Some(hidden_at) = self.ui_hidden_at.get() else {
            return;
        };
        let protected = self.ui_resource_protected();
        if !protected {
            self.release_heavy_page();
        }
        let action = self
            .ui_lease
            .borrow_mut()
            .apply(LeaseEvent::Hidden { protected }, &HiddenAt(hidden_at));
        self.run_lease_action(action);
        self.sample_ui_resources();
    }

    pub(in crate::desktop) fn ui_shown(&self) {
        self.hot_lease_timer.stop();
        self.ui_hidden_at.set(None);
        self.ui_lease.borrow_mut().apply(LeaseEvent::Shown, &Uptime);
    }

    /// 唤醒/每个内核事件/重开前检查含睡眠的绝对期限；不是依赖暂停的 Timer 时间。
    pub(in crate::desktop) fn check_ui_lease(self: &Rc<Self>) {
        if self.finished.get() || !matches!(self.ui_lease.borrow().state(), LeaseState::Hot { .. })
        {
            return;
        }
        let protected = self.ui_resource_protected();
        let guard_action = self
            .ui_lease
            .borrow_mut()
            .apply(LeaseEvent::Hidden { protected }, &Uptime);
        self.run_lease_action(guard_action);
        if protected {
            self.hot_lease_timer.stop();
            return;
        }
        let generation = self.ui_lease.borrow().expired(&Uptime);
        if let Some(generation) = generation {
            let action = self
                .ui_lease
                .borrow_mut()
                .apply(LeaseEvent::Tick { generation }, &Uptime);
            self.run_lease_action(action);
        }
    }

    fn run_lease_action(self: &Rc<Self>, action: LeaseAction) {
        match action {
            LeaseAction::None => {}
            LeaseAction::Arm {
                deadline,
                generation,
            } => {
                let weak = Rc::downgrade(self);
                self.hot_lease_timer.start(TimerMode::SingleShot, Duration::from_millis(deadline.saturating_sub(uptime_milliseconds())), move || {
                    if let Some(runtime) = weak.upgrade() {
                        // 旧代 Timer 已经排入事件队列也不能销毁新一代主窗。
                        if !matches!(runtime.ui_lease.borrow().state(), LeaseState::Hot { generation: current, .. } if current == generation) { return; }
                        runtime.check_ui_lease();
                        if let LeaseState::Hot { deadline, generation: current } = runtime.ui_lease.borrow().state() {
                            if current == generation && !runtime.ui_resource_protected() {
                                runtime.run_lease_action(LeaseAction::Arm { deadline, generation });
                            }
                        }
                    }
                });
                tracing::info!(
                    phase = "ui_lease_armed",
                    lease_generation = generation,
                    deadline_tick_ms = deadline,
                    lifecycle_state = "hot"
                );
            }
            LeaseAction::Release(reason) => {
                if !self.release_ui(reason) {
                    // 同步投影若引入了保护，保守恢复保护态；绝不谎报已进入冷驻留。
                    self.ui_lease
                        .borrow_mut()
                        .apply(LeaseEvent::Mounted, &Uptime);
                    self.ui_lease
                        .borrow_mut()
                        .apply(LeaseEvent::Hidden { protected: true }, &Uptime);
                    tracing::warn!(phase = "ui_release_blocked", lifecycle_state = "protected");
                }
            }
        }
    }

    /// P5：复用已有 30 秒内核 Tick，隐藏后也立即采样一次；不清空进程工作集。
    pub(in crate::desktop) fn sample_ui_resources(self: &Rc<Self>) {
        if self.finished.get() {
            return;
        }
        self.check_ui_lease();
        let sample = match process_resources() {
            Ok(sample) => sample,
            Err(error) => {
                tracing::warn!(
                    phase = "resource_sample",
                    native_code = error.native_code(),
                    error_kind = error.diagnostic_kind()
                );
                return;
            }
        };
        // 这是触发驱逐前的样本，必须按采样当时的状态记录，不能用旧热态数值伪报 cold。
        self.log_resources(sample);
        if let Some(reason) = pressure_reason(sample) {
            let protected = self.ui_resource_protected();
            if !self.main_visible() && !protected && self.with_ui(|_| ()).is_some() {
                self.release_heavy_page();
            }
            let action = self
                .ui_lease
                .borrow_mut()
                .apply(LeaseEvent::Evict { reason, protected }, &Uptime);
            self.run_lease_action(action);
        }
    }

    fn log_resources(&self, sample: ProcessResources) {
        let state = if self.ui_resource_protected()
            && !self.main_visible()
            && self.with_ui(|_| ()).is_some()
        {
            "protected"
        } else {
            self.ui_lease.borrow().label()
        };
        let budget = if state == "cold" {
            COLD_BUDGET_BYTES
        } else {
            HOT_BUDGET_BYTES
        };
        tracing::info!(
            phase = "resource_sample",
            lifecycle_state = state,
            ui_present = self.with_ui(|_| ()).is_some(),
            working_set_bytes = sample.working_set_bytes,
            private_bytes = sample.private_bytes,
            handle_count = sample.handles,
            gdi_objects = sample.gdi_objects,
            user_objects = sample.user_objects,
            system_load_percent = sample.system_load_percent,
            over_budget = sample.working_set_bytes > budget
        );
    }

    /// 页面重数据优先于外壳释放，且不改路由/日期/分页/折叠或必要 DB 服务。
    pub(in crate::desktop) fn release_heavy_page(&self) -> bool {
        if !self.tasks.borrow_mut().release_clean_cache() {
            return false;
        }
        self.pending_effects.borrow_mut().discard_page_effects();
        let mut epoch = self.ui_epoch.get();
        epoch.invalidate_page();
        self.ui_epoch.set(epoch);
        self.today_dirty.set(true);
        self.detail_dirty.set(true);
        let _ =
            self.with_ui(|ui| while_syncing(&self.task_syncing, || ui.release_page_resources()));
        tracing::info!(
            phase = "page_resources_released",
            ui_present = self.with_ui(|_| ()).is_some(),
            query_scope = "today"
        );
        true
    }

    /// 解除唯一 UI 持有。到期/压力不能强制丢草稿；完整退出是已确认的例外。
    pub(in crate::desktop) fn release_ui(&self, reason: ReleaseReason) -> bool {
        let exit = reason == ReleaseReason::Exit;
        if !exit && (self.main_visible() || self.ui_resource_protected()) {
            return false;
        }
        if self.with_ui(|_| ()).is_none() {
            return true;
        }
        if !exit {
            let feedback = self
                .with_ui(|ui| ui.shell.take_retained_feedback())
                .flatten();
            let Some(feedback) = feedback else {
                return false;
            };
            *self.retained_feedback.borrow_mut() = Some(feedback);
        }
        self.hot_lease_timer.stop();
        self.geometry_timer.stop();
        self.pending_effects.borrow_mut().discard_page_effects();
        let mut epoch = self.ui_epoch.get();
        let generation = epoch.session().unwrap_or(0);
        epoch.unmount();
        self.ui_epoch.set(epoch);
        let ui = self.ui.borrow_mut().take();
        let probe = ui.as_ref().map(|ui| ui.shell.component().as_weak());
        drop(ui);
        if exit {
            self.tasks.borrow_mut().today.take();
            self.tasks.borrow_mut().editor.take();
            self.retained_feedback.borrow_mut().take();
            self.journal_save_timer.stop();
            *self.journal.borrow_mut() = super::journal::JournalSession::default();
        } else {
            self.tasks.borrow_mut().release_clean_cache();
        }
        self.today_dirty.set(true);
        self.detail_dirty.set(true);
        self.main_icon.set(0);
        self.main_caption.set(0);
        self.main_frame.set(None);
        let native = self.main_native.take();
        self.ui_hidden_at.set(None);
        // 探针只持有弱组件和旧 HWND 数值。当前事件临时持有的引用也结束后再检查。
        Timer::single_shot(Duration::ZERO, move || {
            tracing::info!(
                phase = "ui_release_probe",
                session_generation = generation,
                component_released = probe.is_none_or(|weak| weak.upgrade().is_none()),
                old_hwnd_present = native.is_some_and(window_belongs_to_process)
            );
        });
        tracing::info!(phase = "ui_session_released", session_generation = generation, release_reason = ?reason, lifecycle_state = "cold", ui_present = false);
        true
    }

    pub(in crate::desktop) fn failed_ui_show(&self) {
        if self.main_surface_reachable() {
            return;
        }
        if self.presentation.get() == super::presentation::Presentation::Main {
            self.presentation
                .set(super::presentation::Presentation::Hidden);
        }
        let protected = self.ui_resource_protected();
        let action = self.ui_lease.borrow_mut().apply(
            LeaseEvent::Discard {
                reason: ReleaseReason::ShowFailure,
                protected,
            },
            &Uptime,
        );
        if let LeaseAction::Release(reason) = action {
            if !self.release_ui(reason) {
                self.ui_mounted();
            }
        }
    }
}

#[cfg(test)]
mod tests;
