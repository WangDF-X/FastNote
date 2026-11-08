use chrono::{NaiveDate, NaiveDateTime};

use super::TaskSummary;

/// 今日四区的稳定顺序，任务只命中一区（ARCH-03 §8.2）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum TodaySectionKind {
    /// 已到期、已开始或进行中的当前关注任务。
    Focus = 0,
    /// 未设日期的未开始任务。
    Unscheduled,
    /// 本地今日完成。
    CompletedToday,
    /// 未开始且日期均在未来的稍后任务。
    Later,
}

impl TodaySectionKind {
    /// 按优先级排列的所有分组。
    pub const ALL: [Self; 4] = [
        Self::Focus,
        Self::Unscheduled,
        Self::CompletedToday,
        Self::Later,
    ];

    /// 解析 UI 边界的组索引。
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// tasks.default_sort，只影响当前关注区，始终保持逾期置顶。
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
    /// ARCH-04 已登记的持久化值；不把 UI 标签写入数据库。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::DueDate => "due_date",
            Self::CreatedDescending => "created_desc",
        }
    }

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
    /// 当前关注排序，逾期置顶不受此选项影响。
    pub sort: TodaySort,
    /// 显示待安排。
    pub show_unscheduled: bool,
    /// 显示今日已完成。
    pub show_completed: bool,
    /// 稍后默认折叠，继承旧明日折叠偏好。
    pub later_collapsed: bool,
}

impl Default for TodayPreferences {
    fn default() -> Self {
        Self {
            sort: TodaySort::Priority,
            show_unscheduled: true,
            show_completed: true,
            later_collapsed: true,
        }
    }
}

impl TodayPreferences {
    /// 命中分组后再判断可见性，不将隐藏任务转移到别组。
    pub fn enabled(self, kind: TodaySectionKind) -> bool {
        match kind {
            TodaySectionKind::Unscheduled => self.show_unscheduled,
            TodaySectionKind::CompletedToday => self.show_completed,
            TodaySectionKind::Focus | TodaySectionKind::Later => true,
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
}
