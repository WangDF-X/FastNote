use std::cell::Cell;

use fastnote_core::TaskClock;

use crate::clock::{ClockCheck, ClockTracker, SystemClock};
use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, TaskSession};

/// 时间检查只更新轻量内核状态，不调用窗口；已失效的期限不会每次 tick 重复触发。
fn observe_invalidation(
    tracker: &mut ClockTracker,
    clock: &impl TaskClock,
    tasks: &mut TaskSession,
    today_dirty: &Cell<bool>,
    detail_dirty: &Cell<bool>,
) -> ClockCheck {
    let mut check = tracker.observe(clock);
    check.changed |= !today_dirty.get() && tasks.needs_clock_refresh(check.sample.local);
    if check.changed {
        today_dirty.set(true);
        detail_dirty.set(true);
        tasks.invalidate_today_read();
    }
    check
}

impl Runtime {
    /// SHELL-03 T2.4：隐藏期间仍观察日期/时区，必要读取由可见页入口另行决定。
    pub(in crate::desktop) fn observe_task_clock(&self) -> ClockCheck {
        observe_invalidation(
            &mut self.task_clock.borrow_mut(),
            &SystemClock,
            &mut self.tasks.borrow_mut(),
            &self.today_dirty,
            &self.detail_dirty,
        )
    }

    /// 内核 tick 不向不可见页面请求或投影；可见今日页失效时只安排必要补读。
    pub(in crate::desktop) fn refresh_task_clock(&self) {
        if self.finished.get() {
            return;
        }
        let check = self.observe_task_clock();
        if !check.changed || !self.ready.get() || self.closing.get() || !self.today_page_visible() {
            return;
        }
        self.request_today();
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .filter(|editor| editor.in_flight.is_none())
            .map(|editor| editor.detail.task.id);
        if let Some(id) = selected {
            self.request_task_detail(DetailPurpose::Refresh(id));
        }
    }
}

#[cfg(test)]
mod tests;
