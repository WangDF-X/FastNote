use thiserror::Error;

/// tasks.kind；阶段不参与任务状态统计。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    /// 可执行任务。
    Task,
    /// 计划根下的阶段。
    Stage,
}

/// 四种互不限制转换的执行状态（ARCH-03 §4.2）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStatus {
    /// 待处理。
    Todo,
    /// 进行中。
    InProgress,
    /// 等待中。
    Waiting,
    /// 已完成。
    Done,
}

impl TaskStatus {
    /// 持久化使用的稳定枚举值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Waiting => "waiting",
            Self::Done => "done",
        }
    }

    /// 解析持久化/动作边界，不接受未知状态。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "todo" => Some(Self::Todo),
            "in_progress" => Some(Self::InProgress),
            "waiting" => Some(Self::Waiting),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// 优先级数值越大越靠前（ARCH-03 §4.3）。
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum Priority {
    /// 无优先级。
    #[default]
    None = 0,
    /// 低。
    Low = 1,
    /// 中。
    Medium = 2,
    /// 高。
    High = 3,
}

impl Priority {
    /// 解析数据库/选择器数值。
    pub fn parse(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Low),
            2 => Some(Self::Medium),
            3 => Some(Self::High),
            _ => None,
        }
    }

    /// 行徽标循环：高 → 中 → 低 → 无 → 高。
    pub fn next_priority(self) -> Self {
        match self {
            Self::High => Self::Medium,
            Self::Medium => Self::Low,
            Self::Low => Self::None,
            Self::None => Self::High,
        }
    }
}

/// 稳定领域错误；不携带标题、描述或原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskError {
    /// 空标题。
    #[error("E_TITLE_REQUIRED")]
    TitleRequired,
    /// 标题超过 200 字符。
    #[error("E_TITLE_TOO_LONG")]
    TitleTooLong,
    /// 描述超过 10,000 字符。
    #[error("E_DESC_TOO_LONG")]
    DescriptionTooLong,
    /// 日期格式、范围或先后关系非法。
    #[error("E_DATE_RANGE")]
    DateRange,
    /// 实体已不存在。
    #[error("E_NOT_FOUND")]
    NotFound,
    /// 版本、唯一约束或实体不变量冲突。
    #[error("E_CONFLICT")]
    Conflict,
    /// 归档计划只读。
    #[error("E_PLAN_ARCHIVED")]
    PlanArchived,
    /// 阶段字段/层级非法。
    #[error("E_STAGE_PARENT_INVALID")]
    StageParentInvalid,
    /// 删除子树前必须明确确认。
    #[error("E_HAS_CHILDREN")]
    HasChildren,
}

/// 可经明确确认继续执行的提示（ARCH-03 §10.2）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarningCode {
    /// 未完成的前置依赖。
    DependencyIncomplete,
    /// 父任务仍有未完成子任务。
    ParentIncomplete,
}
