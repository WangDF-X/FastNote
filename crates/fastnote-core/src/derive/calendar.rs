//! 日历日期聚合的纯领域规则（ARCH-03 §8.3）。

use chrono::NaiveDate;

use crate::{TaskClock, TaskKind, TaskStatus, TaskSummary};

mod navigation;
pub use navigation::{calendar_month_days, calendar_week_days, WeekStart};

/// 某日任务详情；每个任务最多出现在一个分组。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DayAgenda {
    /// 到期任务。
    pub due: Vec<TaskSummary>,
    /// 当日开始但不在到期分组的任务。
    pub start: Vec<TaskSummary>,
    /// 跨天进行中的任务。
    pub ongoing: Vec<TaskSummary>,
    /// 当日本地时间完成的任务。
    pub completed: Vec<TaskSummary>,
    /// 未完成安排超过八项。
    pub overload: bool,
}

/// 基于注入的时区转换生成某日分组；归档计划和阶段不参与。
pub fn build_day_agenda(
    date: NaiveDate,
    tasks: impl IntoIterator<Item = TaskSummary>,
    clock: &impl TaskClock,
) -> DayAgenda {
    let mut agenda = DayAgenda::default();
    for task in tasks {
        if task.kind != TaskKind::Task || task.context.read_only {
            continue;
        }
        match task.status {
            None => continue,
            Some(TaskStatus::Done) => {
                if task
                    .completed_at
                    .is_some_and(|at| clock.local_date(at) == date)
                {
                    agenda.completed.push(task);
                }
            }
            Some(_) if task.due_date == Some(date) => agenda.due.push(task),
            Some(_) if task.start_date == Some(date) => agenda.start.push(task),
            Some(TaskStatus::InProgress)
                if task.start_date.is_some_and(|start| start < date)
                    && task.due_date.is_none_or(|due| due >= date) =>
            {
                agenda.ongoing.push(task);
            }
            Some(_) => {}
        }
    }
    let compare = |a: &TaskSummary, b: &TaskSummary| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.due_date.is_none().cmp(&b.due_date.is_none()))
            .then_with(|| a.due_date.cmp(&b.due_date))
            .then_with(|| a.start_date.is_none().cmp(&b.start_date.is_none()))
            .then_with(|| a.start_date.cmp(&b.start_date))
            .then_with(|| a.created_at.cmp(&b.created_at))
    };
    agenda.due.sort_by(compare);
    agenda.start.sort_by(compare);
    agenda.ongoing.sort_by(|a, b| {
        a.start_date
            .cmp(&b.start_date)
            .then_with(|| b.priority.cmp(&a.priority))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    agenda
        .completed
        .sort_by_key(|task| std::cmp::Reverse(task.completed_at));
    agenda.overload = agenda.due.len() + agenda.start.len() + agenda.ongoing.len() > 8;
    agenda
}

#[cfg(test)]
mod tests;
