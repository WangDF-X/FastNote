use chrono::{DateTime, NaiveDate, NaiveTime, Utc};

use crate::{PlanId, Priority, Task, TaskId, TaskKind, TaskStatus};

/// 仓储一次查询得到的关联信息，不包含额外业务副本。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaskContext {
    /// 所属计划名称。
    pub plan_name: Option<String>,
    /// 所属阶段名称。
    pub stage_name: Option<String>,
    /// 直接子任务数量。
    pub child_total: u32,
    /// 已完成直接子任务数量。
    pub child_done: u32,
    /// 所有后代任务数量，删除确认使用。
    pub descendant_count: u32,
    /// 前置依赖数量。
    pub dependency_count: u32,
    /// 未完成前置数量。
    pub blocking_count: u32,
    /// 今日列表最多十个未完成前置任务标题，按标题和 ID 稳定排序。
    pub blocking_titles: Vec<String>,
    /// 所属计划已归档。
    pub read_only: bool,
}

/// 今日、日历和计划板可复用的任务行读模型；不读取长描述。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskSummary {
    /// 主键。
    pub id: TaskId,
    /// 节点种类。
    pub kind: TaskKind,
    /// 标题。
    pub title: String,
    /// 执行状态。
    pub status: Option<TaskStatus>,
    /// 优先级。
    pub priority: Priority,
    /// 开始日期。
    pub start_date: Option<NaiveDate>,
    /// 开始时分；旧全天任务为空。
    pub start_time: Option<NaiveTime>,
    /// 到期日期。
    pub due_date: Option<NaiveDate>,
    /// 到期时分；旧全天任务为空。
    pub due_time: Option<NaiveTime>,
    /// 所属计划。
    pub plan_id: Option<PlanId>,
    /// 父节点。
    pub parent_id: Option<TaskId>,
    /// 关联名称与计数。
    pub context: TaskContext,
    /// core 统一计算的逾期标记。
    pub is_overdue: bool,
    /// core 统一计算的阻塞标记。
    pub is_blocked: bool,
    /// 完成时间。
    pub completed_at: Option<DateTime<Utc>>,
    /// 创建时间；当前关注、待安排及稍后排序使用。
    pub created_at: DateTime<Utc>,
    /// 最近状态变化；保留历史时间，不随分区调整。
    pub status_changed_at: DateTime<Utc>,
    /// 最近变更。
    pub updated_at: DateTime<Utc>,
}

impl TaskSummary {
    /// 投影实体与关联信息；派生状态随后由统一规则填充。
    pub fn from_task(task: &Task, context: TaskContext) -> Self {
        Self::with_title(task, context, task.title.clone())
    }

    /// 消费不再需要的实体，移动标题以避免列表查询中的额外字符串分配。
    pub fn from_owned_task(mut task: Task, context: TaskContext) -> Self {
        let title = std::mem::take(&mut task.title);
        Self::with_title(&task, context, title)
    }

    fn with_title(task: &Task, context: TaskContext, title: String) -> Self {
        Self {
            id: task.id,
            kind: task.kind,
            title,
            status: task.status,
            priority: task.priority,
            start_date: task.start_date,
            start_time: task.start_time,
            due_date: task.due_date,
            due_time: task.due_time,
            plan_id: task.plan_id,
            parent_id: task.parent_id,
            context,
            is_overdue: false,
            is_blocked: false,
            completed_at: task.completed_at,
            created_at: task.created_at,
            status_changed_at: task.status_changed_at,
            updated_at: task.updated_at,
        }
    }
}

/// 全局共享详情，实体与只读/关联信息来自同一查询。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskDetail {
    /// 完整实体，含描述。
    pub task: Task,
    /// 计划/子任务/依赖信息。
    pub context: TaskContext,
    /// 与列表一致的逾期标记。
    pub is_overdue: bool,
    /// 与列表一致的阻塞标记。
    pub is_blocked: bool,
}
