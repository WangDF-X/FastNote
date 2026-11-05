use std::cmp::Ordering;
use std::collections::BinaryHeap;

use chrono::{NaiveDate, NaiveDateTime};

use crate::{
    TaskClock, TaskKind, TaskStatus, TaskSummary, TodayPreferences, TodaySection,
    TodaySectionKind as Kind, TodaySort, TodayView,
};

use super::{compare, derive_at, section_at};

struct RankedTask {
    task: TaskSummary,
    kind: Kind,
    sort: TodaySort,
}

impl Ord for RankedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.kind as usize)
            .cmp(&(other.kind as usize))
            .then((self.sort as u8).cmp(&(other.sort as u8)))
            .then_with(|| compare(&self.task, &other.task, self.kind, self.sort))
            .then_with(|| self.task.id.cmp(&other.task.id))
    }
}
impl PartialOrd for RankedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for RankedTask {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for RankedTask {}

/// 查询层逐行组装今日视图；每个启用组只保留排序最前的 200–1000 条。
/// 完整数量/最近到期边界仍覆盖全部候选，不先分配全量 Vec 后截断。
pub struct TodayCollector<'a, C: TaskClock> {
    clock: &'a C,
    evaluated_at: NaiveDateTime,
    preferences: TodayPreferences,
    limits: [usize; 8],
    groups: [BinaryHeap<RankedTask>; 8],
    totals: [u32; 8],
    next_overdue_at: Option<NaiveDateTime>,
}

impl<'a, C: TaskClock> TodayCollector<'a, C> {
    /// 同一读取只采样一次业务时钟，沿用 core 的分组/派生/排序规则。
    pub fn new(
        today: NaiveDate,
        clock: &'a C,
        preferences: TodayPreferences,
        limits: [u32; 8],
    ) -> Self {
        let limits = limits.map(|limit| limit.clamp(200, 1000) as usize);
        Self {
            clock,
            evaluated_at: today.and_time(clock.local_datetime(clock.now()).time()),
            preferences,
            limits,
            groups: std::array::from_fn(|index| {
                BinaryHeap::with_capacity(if preferences.enabled(Kind::ALL[index]) {
                    limits[index]
                } else {
                    0
                })
            }),
            totals: [0; 8],
            next_overdue_at: None,
        }
    }

    /// 消费单个无长正文的候选；只保留有界排序前缀，其余立即释放。
    pub fn push(&mut self, mut task: TaskSummary) {
        if task.kind == TaskKind::Task
            && !task.context.read_only
            && task.status.is_some_and(|status| status != TaskStatus::Done)
        {
            if let Some(deadline) = task
                .due_date
                .zip(task.due_time)
                .map(|(date, time)| date.and_time(time))
                .filter(|deadline| *deadline >= self.evaluated_at)
            {
                self.next_overdue_at = Some(
                    self.next_overdue_at
                        .map_or(deadline, |previous| previous.min(deadline)),
                );
            }
        }
        derive_at(&mut task, self.evaluated_at, self.clock);
        let Some(kind) = section_at(&task, self.evaluated_at, self.clock) else {
            return;
        };
        let index = kind as usize;
        self.totals[index] = self.totals[index].saturating_add(1);
        if !self.preferences.enabled(kind) {
            return;
        }
        let ranked = RankedTask {
            task,
            kind,
            sort: self.preferences.sort,
        };
        let group = &mut self.groups[index];
        if group.len() < self.limits[index] {
            group.push(ranked);
        } else if group.peek().is_some_and(|worst| ranked < *worst) {
            group.pop();
            group.push(ranked);
        }
    }

    /// 移动排序前缀为读模型，不复制任务；计数不受分页或隐藏组影响。
    pub fn finish(self) -> TodayView {
        let mut view = TodayView {
            date: self.evaluated_at.date(),
            evaluated_at: self.evaluated_at,
            next_overdue_at: self.next_overdue_at,
            sections: Vec::with_capacity(8),
            open_count: 0,
            overdue_count: self.totals[Kind::Overdue as usize],
            completed_today_count: self.totals[Kind::CompletedToday as usize],
            tomorrow_count: self.totals[Kind::Tomorrow as usize],
        };
        for (kind, group) in Kind::ALL.into_iter().zip(self.groups) {
            let total = self.totals[kind as usize];
            if total == 0 || !self.preferences.enabled(kind) {
                continue;
            }
            if kind != Kind::CompletedToday {
                view.open_count = view.open_count.saturating_add(total);
            }
            view.sections.push(TodaySection {
                kind,
                items: group
                    .into_sorted_vec()
                    .into_iter()
                    .map(|ranked| ranked.task)
                    .collect(),
                total,
                has_more: total as usize > self.limits[kind as usize],
                collapsed_default: kind == Kind::CompletedToday
                    || (kind == Kind::Tomorrow && self.preferences.tomorrow_collapsed),
            });
        }
        view
    }
}

#[cfg(test)]
mod tests;
