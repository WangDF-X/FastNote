use crate::{DateTime, NaiveDate, TaskId, TaskPatch, TaskStatus, Utc};

/// 首个业务里程碑的显式任务命令；由应用门面投递到唯一 DB worker。
#[derive(Clone, Debug)]
pub enum Command {
    /// 快速创建独立任务，ID 在应用层生成。
    CreateTask { id: TaskId, title: String },
    /// 从日历创建独立任务，选中日作为开始日期，不设置到期日期。
    CreateCalendarTask {
        id: TaskId,
        title: String,
        date: NaiveDate,
    },
    /// 标题/描述/日期/优先级的原子补丁。
    UpdateTask {
        id: TaskId,
        patch: TaskPatch,
        expected_updated_at: DateTime<Utc>,
    },
    /// 四态转换；confirmed 仅来自明确的警告确认。
    SetTaskStatus {
        id: TaskId,
        status: TaskStatus,
        confirmed: bool,
    },
    /// 删除任务或已明确确认的子树。
    DeleteTask { id: TaskId, cascade: bool },
}
