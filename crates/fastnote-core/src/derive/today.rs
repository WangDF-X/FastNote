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

/// 使用注入的本地时间转换填充统一标记，不让 UI 推导等待天数。
pub fn derive_task_summary(task: &mut TaskSummary, today: NaiveDate, clock: &impl TaskClock) {
    derive_at(
        task,
        today.and_time(clock.local_datetime(clock.now()).time()),
        clock,
    );
}

fn derive_at(task: &mut TaskSummary, now: NaiveDateTime, clock: &impl TaskClock) {
    task.is_overdue = is_overdue(task, now);
    task.is_blocked = task.context.blocking_count > 0;
    task.waiting_days = (task.status == Some(TaskStatus::Waiting)).then(|| {
        u32::try_from(
            (now.date() - clock.local_date(task.status_changed_at))
                .num_days()
                .max(0),
        )
        .unwrap_or(u32::MAX)
    });
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
    let today = now.date();
    if task.kind != TaskKind::Task || task.context.read_only {
        return None;
    }
    let status = task.status?;
    if status == TaskStatus::Done {
        return task
            .completed_at
            .filter(|at| clock.local_date(*at) == today)
            .map(|_| Kind::CompletedToday);
    }
    if is_overdue(task, now) {
        return Some(Kind::Overdue);
    }
    if task.due_date == Some(today) {
        return Some(Kind::DueToday);
    }
    if task.start_date == Some(today) {
        return Some(Kind::StartsToday);
    }
    if status == TaskStatus::InProgress
        && task.start_date.is_none_or(|date| date <= today)
        && task.due_date.is_none_or(|date| date > today)
    {
        return Some(Kind::InProgress);
    }
    if status == TaskStatus::Waiting {
        return Some(Kind::Waiting);
    }
    if status == TaskStatus::Todo && task.start_date.is_none() && task.due_date.is_none() {
        return Some(Kind::Unscheduled);
    }
    today
        .succ_opt()
        .filter(|tomorrow| task.due_date == Some(*tomorrow) || task.start_date == Some(*tomorrow))
        .map(|_| Kind::Tomorrow)
}

/// 统一分组、排序、完整计数和每组 200–1000 条加载上限。
pub fn build_today_view(
    tasks: Vec<TaskSummary>,
    today: NaiveDate,
    clock: &impl TaskClock,
    preferences: TodayPreferences,
    limits: [u32; 8],
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
    match kind {
        Kind::Waiting => a.status_changed_at.cmp(&b.status_changed_at).then(priority),
        Kind::Unscheduled => priority.then_with(|| b.created_at.cmp(&a.created_at)),
        Kind::CompletedToday => b.completed_at.cmp(&a.completed_at),
        Kind::Tomorrow => priority
            .then(due)
            .then(start)
            .then(a.created_at.cmp(&b.created_at)),
        _ => match sort {
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
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod timing_tests;
