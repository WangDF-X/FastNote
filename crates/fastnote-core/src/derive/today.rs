use std::cmp::Ordering;

use chrono::{NaiveDate, NaiveDateTime};

use crate::{
    TaskClock, TaskKind, TaskStatus, TaskSummary, TodayPreferences, TodaySectionKind as Kind,
    TodaySort, TodayView,
};

mod bounded;
pub use bounded::TodayCollector;

/// 无到期日、阶段、已完成或归档任务不会逾期。
pub fn is_overdue(task: &TaskSummary, now: NaiveDateTime) -> bool {
    task.kind == TaskKind::Task
        && !task.context.read_only
        && task.status.is_some_and(|status| status != TaskStatus::Done)
        && task.due_date.is_some_and(|due| match task.due_time {
            Some(time) => due.and_time(time) < now,
            None => due < now.date(),
        })
}

/// 使用注入的本地时间转换填充统一逾期与阻塞标记。
pub fn derive_task_summary(task: &mut TaskSummary, today: NaiveDate, clock: &impl TaskClock) {
    derive_at(
        task,
        today.and_time(clock.local_datetime(clock.now()).time()),
    );
}

fn derive_at(task: &mut TaskSummary, now: NaiveDateTime) {
    task.is_overdue = is_overdue(task, now);
    task.is_blocked = task.context.blocking_count > 0;
}

/// 按 ARCH-03 的优先级去重归组；隐藏设置不参与归组。
pub fn today_section(task: &TaskSummary, today: NaiveDate, clock: &impl TaskClock) -> Option<Kind> {
    section_at(
        task,
        today.and_time(clock.local_datetime(clock.now()).time()),
        clock,
    )
}

fn section_at(task: &TaskSummary, now: NaiveDateTime, clock: &impl TaskClock) -> Option<Kind> {
    if task.kind != TaskKind::Task || task.context.read_only {
        return None;
    }
    let status = task.status?;
    if status == TaskStatus::Done {
        return task
            .completed_at
            .filter(|at| clock.local_date(*at) == now.date())
            .map(|_| Kind::CompletedToday);
    }
    // D-TODAY-09：过去开始的未开始任务不消失，手动进行中不受未来排期限制。
    if is_overdue(task, now)
        || task.start_date.is_some_and(|date| date <= now.date())
        || task.due_date.is_some_and(|date| date <= now.date())
        || status == TaskStatus::InProgress
    {
        Some(Kind::Focus)
    } else if task.start_date.is_none() && task.due_date.is_none() {
        Some(Kind::Unscheduled)
    } else {
        Some(Kind::Later)
    }
}

/// 统一分组、排序、完整计数和每组 200–1000 条加载上限。
pub fn build_today_view(
    tasks: Vec<TaskSummary>,
    today: NaiveDate,
    clock: &impl TaskClock,
    preferences: TodayPreferences,
    limits: [u32; 4],
) -> TodayView {
    let mut collector = TodayCollector::new(today, clock, preferences, limits);
    for task in tasks {
        collector.push(task);
    }
    collector.finish()
}

fn optional_value<T: Ord>(a: Option<T>, b: Option<T>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare(a: &TaskSummary, b: &TaskSummary, kind: Kind, sort: TodaySort) -> Ordering {
    let priority = b.priority.cmp(&a.priority);
    let due =
        optional_value(a.due_date, b.due_date).then_with(|| optional_value(a.due_time, b.due_time));
    let start =
        optional_value(a.start_date, b.start_date).then_with(|| a.start_time.cmp(&b.start_time));
    let ordering = match kind {
        Kind::Unscheduled => priority.then_with(|| b.created_at.cmp(&a.created_at)),
        Kind::CompletedToday => b.completed_at.cmp(&a.completed_at),
        Kind::Later => priority
            .then(due)
            .then(start)
            .then(a.created_at.cmp(&b.created_at)),
        Kind::Focus => match sort {
            TodaySort::Priority => priority
                .then(due)
                .then(start)
                .then(a.created_at.cmp(&b.created_at)),
            TodaySort::DueDate => due
                .then(priority)
                .then(start)
                .then(a.created_at.cmp(&b.created_at)),
            TodaySort::CreatedDescending => b.created_at.cmp(&a.created_at),
        },
    };
    if kind == Kind::Focus {
        b.is_overdue.cmp(&a.is_overdue).then(ordering)
    } else {
        ordering
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod timing_tests;
