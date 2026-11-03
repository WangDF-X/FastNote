use crate::{NoteId, PlanId, TaskId, WarningCode};

/// 单一业务对象的提醒所有者，通知模块后续复用。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReminderOwner {
    /// 任务提醒。
    Task(TaskId),
    /// 小记提醒。
    Note(NoteId),
}

/// 任务变化应刷新哪个读模型（ARCH-02 §4.3）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskScope {
    /// 同一任务变化可能影响其他任务的阻塞/子任务统计，所有已打开读模型重查。
    All,
    /// 今日列表。
    Today,
    /// 日历。
    Calendar,
    /// 计划树。
    Plan(PlanId),
    /// 指定任务详情。
    Detail(TaskId),
}

/// 仅在写事务成功后产生的领域变更通知，不携带正文。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEvent {
    /// 任务数据变化。
    TasksChanged {
        scope: TaskScope,
        affected: Vec<TaskId>,
    },
    /// 完成/删除同步改变提醒资格。
    RemindersChanged { owners: Vec<ReminderOwner> },
    /// 删除可能解除小记与任务的外键关联。
    NotesChanged,
}

/// 命令结果及其已提交事件；需要确认时不能有写入或变更事件。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandResult {
    /// 已变化的任务。
    pub affected_tasks: Vec<TaskId>,
    /// 受影响的计划。
    pub affected_plans: Vec<PlanId>,
    /// 成功提交后供各页面重查的事件。
    pub events: Vec<AppEvent>,
    /// 必须展示的操作警告。
    pub warnings: Vec<WarningCode>,
    /// 尚未写入，等待用户确认。
    pub needs_confirmation: bool,
}
