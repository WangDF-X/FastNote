use std::rc::Rc;
use std::time::Duration;

use fastnote_core::TaskClock;
use slint::{Timer, TimerMode};

use crate::clock::SystemClock;
use crate::desktop::runtime::Runtime;
use crate::tasks::DetailPurpose;

use super::super::policy::{needs_page_load, today_page_visible};

impl Runtime {
    /// DB 就绪后只启动常驻日期检测；正常隐藏启动不请求业务页。
    pub(in crate::desktop) fn start_kernel_tasks(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.task_day_timer
            .start(TimerMode::Repeated, Duration::from_secs(30), move || {
                if let Some(runtime) = weak.upgrade() {
                    let changed = runtime
                        .tasks
                        .borrow()
                        .needs_clock_refresh(SystemClock.local_datetime(SystemClock.now()));
                    if changed {
                        runtime.request_today();
                        let selected = runtime
                            .tasks
                            .borrow()
                            .editor
                            .as_ref()
                            .map(|editor| editor.detail.task.id);
                        if let Some(id) = selected {
                            runtime.request_task_detail(DetailPurpose::Refresh(id));
                        }
                    }
                }
            });
    }

    /// 当前页面真正可见后才补读；重复显示和原生恢复共用同一在途门控。
    pub(in crate::desktop) fn start_page_tasks(&self) {
        if !self.ready.get()
            || self.finished.get()
            || self.closing.get()
            || !self.main_content_visible()
        {
            return;
        }
        self.ensure_current_page_resources();
        if !self.today_page_visible() {
            return;
        }
        let needed = {
            let tasks = self.tasks.borrow();
            needs_page_load(
                self.today_dirty.get(),
                tasks.today.is_some(),
                tasks.latest_today,
            )
        };
        if needed {
            self.request_today();
        }
        self.sync_task_editor();
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .filter(|editor| editor.in_flight.is_none())
            .map(|editor| editor.detail.task.id);
        if self.detail_dirty.get() {
            if let Some(id) = selected {
                self.request_task_detail(DetailPurpose::Refresh(id));
            }
        }
    }

    /// 外壳会在动作返回后投影路由，不能在旧页面仍显示时提前读取新页面。
    pub(in crate::desktop) fn defer_page_tasks(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                runtime.start_page_tasks();
            }
        });
    }

    pub(in crate::desktop) fn today_page_visible(&self) -> bool {
        let page = self.controller.borrow().shell_view().page_label;
        self.with_ui(|ui| {
            today_page_visible(
                self.main_content_visible(),
                page,
                ui.shell.component().get_current_page().as_str(),
            ) && ui.today_presenter.borrow().is_some()
        })
        .unwrap_or(false)
    }

    /// 隐藏只失效、取消普通打开/刷新意图；必要保存/删除确认读仍由内核接收。
    pub(in crate::desktop) fn task_page_hidden(&self) {
        self.today_dirty.set(true);
        self.detail_dirty.set(true);
        {
            let mut tasks = self.tasks.borrow_mut();
            tasks.invalidate_today_read();
            tasks.cancel_page_reads();
        }
        let _ = self.with_ui(|ui| {
            ui.content_active.set(false);
            ui.task_feedback_timer.stop();
            ui.task_saving_visible.set(false);
        });
    }
}

#[cfg(test)]
mod tests;
