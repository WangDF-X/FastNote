use chrono::{NaiveDate, NaiveDateTime};

use super::TaskSummary;

/// 今日分组的稳定顺序，任务只命中一组（ARCH-03 §8.2）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum TodaySectionKind {
    /// 逾期。
    Overdue = 0,
    /// 今日到期。
    DueToday,
    /// 今日开始。
    StartsToday,
    /// 进行中。
    InProgress,
    /// 等待中。
    Waiting,
    /// 未安排。
    Unscheduled,
    /// 明日预告。
    Tomorrow,
    /// 今日已完成。
    CompletedToday,
}

impl TodaySectionKind {
    /// 按优先级排列的所有分组。
    pub const ALL: [Self; 8] = [
        Self::Overdue,
        Self::DueToday,
        Self::StartsToday,
        Self::InProgress,
        Self::Waiting,
        Self::Unscheduled,
        Self::Tomorrow,
        Self::CompletedToday,
    ];

    /// 解析 UI 边界的组索引。
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// tasks.default_sort，只影响四个日期类分组。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TodaySort {
    /// 优先级 → 到期 → 开始 → 创建。
    #[default]
    Priority,
    /// 到期 → 优先级 → 开始 → 创建。
    DueDate,
    /// 创建倒序。
    CreatedDescending,
}

impl TodaySort {
    /// 解析 ARCH-04 已登记的值。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "priority" => Some(Self::Priority),
            "due_date" => Some(Self::DueDate),
            "created_desc" => Some(Self::CreatedDescending),
            _ => None,
        }
    }
}

/// 今日配置；未知键/非法值在仓储边界回退，不新增偏好副本。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TodayPreferences {
    /// 四个日期类分组排序。
    pub sort: TodaySort,
    /// 显示进行中。
    pub show_in_progress: bool,
    /// 显示等待中。
    pub show_waiting: bool,
    /// 显示未安排。
    pub show_unscheduled: bool,
    /// 显示今日已完成。
    pub show_completed: bool,
    /// 明日默认折叠。
    pub tomorrow_collapsed: bool,
}

impl Default for TodayPreferences {
    fn default() -> Self {
        Self {
            sort: TodaySort::Priority,
            show_in_progress: true,
            show_waiting: true,
            show_unscheduled: true,
            show_completed: true,
            tomorrow_collapsed: true,
        }
    }
}

impl TodayPreferences {
    /// 命中分组后再判断可见性，不将隐藏任务转移到别组。
    pub fn enabled(self, kind: TodaySectionKind) -> bool {
        match kind {
            TodaySectionKind::InProgress => self.show_in_progress,
            TodaySectionKind::Waiting => self.show_waiting,
            TodaySectionKind::Unscheduled => self.show_unscheduled,
            TodaySectionKind::CompletedToday => self.show_completed,
            _ => true,
        }
    }
}

/// 一组已排序、分页后的只读任务。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodaySection {
    /// 分组种类。
    pub kind: TodaySectionKind,
    /// 当前加载条目。
    pub items: Vec<TaskSummary>,
    /// 默认折叠；会话可覆盖。
    pub collapsed_default: bool,
    /// 未分页的完整数量。
    pub total: u32,
    /// 当前还有未加载项。
    pub has_more: bool,
}

/// 今日视图由 core 统一组装，统计不受折叠/分页影响。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodayView {
    /// 本地自然日。
    pub date: NaiveDate,
    /// 本次分组采用的本地时刻，用于时钟回拨后重新计算。
    pub evaluated_at: NaiveDateTime,
    /// 完整候选集最近的未来到期时刻，不受折叠、隐藏或分页影响。
    pub next_overdue_at: Option<NaiveDateTime>,
    /// 非空且启用的分组。
    pub sections: Vec<TodaySection>,
    /// 启用的未完成组完整数量。
    pub open_count: u32,
    /// 逾期数量。
    pub overdue_count: u32,
    /// 本地今日完成数量。
    pub completed_today_count: u32,
    /// 明日预告数量。
    pub tomorrow_count: u32,
}
