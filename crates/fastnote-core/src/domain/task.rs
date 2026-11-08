//! 统一任务实体、字段补丁和三态转换（ARCH-03 §3–7）。

use chrono::{DateTime, NaiveDate, NaiveTime, Timelike, Utc};

use crate::{PlanId, TaskId};

use super::{Priority, TaskError, TaskKind, TaskStatus};

/// 任务/阶段的持久实体；页面使用 DTO，不各自维护业务副本。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    /// UUIDv7 主键，由应用层生成。
    pub id: TaskId,
    /// 所属计划；独立任务为空。
    pub plan_id: Option<PlanId>,
    /// 同一计划/独立树内的父节点。
    pub parent_id: Option<TaskId>,
    /// 任务或阶段。
    pub kind: TaskKind,
    /// 去首尾空白后 1–200 个 Unicode 字符。
    pub title: String,
    /// 原样保留的纯文本，最多 10,000 字符。
    pub description: String,
    /// 阶段为 None，任务必须有三态之一。
    pub status: Option<TaskStatus>,
    /// 优先级；阶段为 None。
    pub priority: Priority,
    /// 可选的计划开始日期。
    pub start_date: Option<NaiveDate>,
    /// 开始日期的设备本地时分；旧全天任务为空。
    pub start_time: Option<NaiveTime>,
    /// 可选的计划完成日期。
    pub due_date: Option<NaiveDate>,
    /// 到期日期的设备本地时分；旧全天任务为空。
    pub due_time: Option<NaiveTime>,
    /// 当且仅当任务为 done 时存在。
    pub completed_at: Option<DateTime<Utc>>,
    /// 最近一次真实状态变化时间。
    pub status_changed_at: DateTime<Utc>,
    /// 同父节点内的正整数排序值。
    pub sort_order: i64,
    /// 创建时间（UTC）。
    pub created_at: DateTime<Utc>,
    /// 最近字段变更时间（UTC）。
    pub updated_at: DateTime<Utc>,
}

/// 区分不修改、设置和清空可选字段（ARCH-03 §7.2）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Patch<T> {
    /// 保持原值。
    #[default]
    Keep,
    /// 设置新值。
    Set(T),
    /// 清空。
    Clear,
}

impl<T: Clone> Patch<T> {
    fn apply(&self, value: &mut Option<T>) {
        match self {
            Self::Keep => {}
            Self::Set(new) => *value = Some(new.clone()),
            Self::Clear => *value = None,
        }
    }
}

/// 本轮共享详情可编辑字段；状态单独走 SetTaskStatus，移动走后续专用命令。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaskPatch {
    /// None 表示不修改标题。
    pub title: Option<String>,
    /// None 表示不修改描述。
    pub description: Option<String>,
    /// None 表示不修改优先级。
    pub priority: Option<Priority>,
    /// 开始日期补丁。
    pub start_date: Patch<NaiveDate>,
    /// 开始时分补丁；清除开始日期也会清除时分。
    pub start_time: Patch<NaiveTime>,
    /// 到期日期补丁。
    pub due_date: Patch<NaiveDate>,
    /// 到期时分补丁；清除到期日期也会清除时分。
    pub due_time: Patch<NaiveTime>,
}

impl Task {
    /// 构造只要求标题的独立未开始任务；不读取时钟、不生成 ID。
    pub fn new(id: TaskId, title: String, now: DateTime<Utc>) -> Result<Self, TaskError> {
        let task = Self {
            id,
            plan_id: None,
            parent_id: None,
            kind: TaskKind::Task,
            title: title.trim().to_owned(),
            description: String::new(),
            status: Some(TaskStatus::Todo),
            priority: Priority::None,
            start_date: None,
            start_time: None,
            due_date: None,
            due_time: None,
            completed_at: None,
            status_changed_at: now,
            sort_order: 1024,
            created_at: now,
            updated_at: now,
        };
        task.validate()?;
        Ok(task)
    }

    /// 检查本实体可独立判断的不变量；树/计划/依赖检查由服务与事务补充。
    pub fn validate(&self) -> Result<(), TaskError> {
        if self.title.trim().is_empty() {
            return Err(TaskError::TitleRequired);
        }
        if self.title.chars().count() > 200 {
            return Err(TaskError::TitleTooLong);
        }
        if self.description.chars().count() > 10_000 {
            return Err(TaskError::DescriptionTooLong);
        }
        for date in [self.start_date, self.due_date].into_iter().flatten() {
            if date < NaiveDate::from_ymd_opt(1900, 1, 1).ok_or(TaskError::DateRange)?
                || date > NaiveDate::from_ymd_opt(9999, 12, 31).ok_or(TaskError::DateRange)?
            {
                return Err(TaskError::DateRange);
            }
        }
        if matches!((self.start_date, self.due_date), (Some(start), Some(due)) if start > due) {
            return Err(TaskError::DateRange);
        }
        // D-TODAY-02：分钟精度、时分必须依附日期；同一天也校验开始/到期先后。
        for (date, time) in [
            (self.start_date, self.start_time),
            (self.due_date, self.due_time),
        ] {
            if time
                .is_some_and(|time| date.is_none() || time.second() != 0 || time.nanosecond() != 0)
            {
                return Err(TaskError::DateRange);
            }
        }
        if self.start_date.is_some()
            && self.start_date == self.due_date
            && matches!((self.start_time, self.due_time), (Some(start), Some(due)) if start > due)
        {
            return Err(TaskError::DateRange);
        }
        match self.kind {
            TaskKind::Stage => {
                if self.plan_id.is_none()
                    || self.parent_id.is_some()
                    || self.status.is_some()
                    || self.priority != Priority::None
                    || self.completed_at.is_some()
                    || self.start_time.is_some()
                    || self.due_time.is_some()
                {
                    return Err(TaskError::StageParentInvalid);
                }
            }
            TaskKind::Task => {
                if self.status.is_none()
                    || (self.status == Some(TaskStatus::Done)) != self.completed_at.is_some()
                {
                    return Err(TaskError::Conflict);
                }
            }
        }
        if self.sort_order <= 0 {
            return Err(TaskError::Conflict);
        }
        Ok(())
    }

    /// 原子校验字段补丁；错误不修改原实体，日期不会联动状态。
    pub fn patched(&self, patch: &TaskPatch, now: DateTime<Utc>) -> Result<Self, TaskError> {
        let mut next = self.clone();
        if let Some(title) = &patch.title {
            next.title = title.trim().to_owned();
        }
        if let Some(description) = &patch.description {
            next.description.clone_from(description);
        }
        if let Some(priority) = patch.priority {
            next.priority = priority;
        }
        patch.start_date.apply(&mut next.start_date);
        patch.due_date.apply(&mut next.due_date);
        if matches!(patch.start_date, Patch::Clear) {
            next.start_time = None;
        }
        if matches!(patch.due_date, Patch::Clear) {
            next.due_time = None;
        }
        patch.start_time.apply(&mut next.start_time);
        patch.due_time.apply(&mut next.due_time);
        next.validate()?;
        if next != *self {
            next.updated_at = now;
        }
        Ok(next)
    }

    /// 三态允许任意转换；重复设置不改变历史时间（INV-12）。
    pub fn with_status(&self, status: TaskStatus, now: DateTime<Utc>) -> Result<Self, TaskError> {
        if self.kind != TaskKind::Task {
            return Err(TaskError::StageParentInvalid);
        }
        let mut next = self.clone();
        if next.status != Some(status) {
            next.status = Some(status);
            next.completed_at = (status == TaskStatus::Done).then_some(now);
            next.status_changed_at = now;
            next.updated_at = now;
        }
        next.validate()?;
        Ok(next)
    }
}

/// 严格解析可清空的 YYYY-MM-DD；非法/越界日期不猜测、不夹取。
pub fn parse_task_date(value: &str) -> Result<Option<NaiveDate>, TaskError> {
    if value.is_empty() {
        return Ok(None);
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| TaskError::DateRange)?;
    if date.format("%Y-%m-%d").to_string() != value
        || date < NaiveDate::from_ymd_opt(1900, 1, 1).ok_or(TaskError::DateRange)?
        || date > NaiveDate::from_ymd_opt(9999, 12, 31).ok_or(TaskError::DateRange)?
    {
        return Err(TaskError::DateRange);
    }
    Ok(Some(date))
}

/// 严格解析可空的 24 小时制 HH:MM；不接受秒、非补零格式或越界时分。
pub fn parse_task_time(value: &str) -> Result<Option<NaiveTime>, TaskError> {
    if value.is_empty() {
        return Ok(None);
    }
    let time = NaiveTime::parse_from_str(value, "%H:%M").map_err(|_| TaskError::DateRange)?;
    if time.format("%H:%M").to_string() != value {
        return Err(TaskError::DateRange);
    }
    Ok(Some(time))
}

#[cfg(test)]
mod tests;
