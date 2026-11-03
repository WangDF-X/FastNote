use fastnote_core::{
    parse_task_date, parse_task_time, DateTime, NaiveDateTime, Patch, Task, TaskDetail, TaskError,
    TaskPatch, Utc,
};

/// 只用于当前输入的本地缓冲；持久状态仍由数据库快照确认。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DraftFields {
    pub title: String,
    pub description: String,
    pub start_date: String,
    pub start_time: String,
    pub due_date: String,
    pub due_time: String,
}

impl DraftFields {
    pub(crate) fn from_task(task: &Task) -> Self {
        Self {
            title: task.title.clone(),
            description: task.description.clone(),
            start_date: task
                .start_date
                .map(|date| date.to_string())
                .unwrap_or_default(),
            start_time: task
                .start_time
                .map(|time| time.format("%H:%M").to_string())
                .unwrap_or_default(),
            due_date: task
                .due_date
                .map(|date| date.to_string())
                .unwrap_or_default(),
            due_time: task
                .due_time
                .map(|time| time.format("%H:%M").to_string())
                .unwrap_or_default(),
        }
    }

    fn patch(&self, base: &Task) -> Result<TaskPatch, TaskError> {
        let start = parse_task_date(&self.start_date)?;
        let due = parse_task_date(&self.due_date)?;
        let start_time = parse_task_time(&self.start_time)?;
        let due_time = parse_task_time(&self.due_time)?;
        let patch = TaskPatch {
            title: (self.title != base.title).then(|| self.title.clone()),
            description: (self.description != base.description).then(|| self.description.clone()),
            start_date: field_patch(base.start_date, start),
            start_time: field_patch(base.start_time, start_time),
            due_date: field_patch(base.due_date, due),
            due_time: field_patch(base.due_time, due_time),
            ..Default::default()
        };
        base.patched(&patch, base.updated_at)?;
        Ok(patch)
    }
}

fn field_patch<T: PartialEq>(old: Option<T>, next: Option<T>) -> Patch<T> {
    if old == next {
        Patch::Keep
    } else {
        next.map(Patch::Set).unwrap_or(Patch::Clear)
    }
}

/// 一个实际提交的输入版本；旧回包只能确认该版本，不能覆盖后续输入。
#[derive(Clone, Debug)]
pub(crate) struct SaveSnapshot {
    pub generation: u64,
    pub patch: TaskPatch,
    pub expected_updated_at: DateTime<Utc>,
}

pub(crate) struct EditorSession {
    pub detail: TaskDetail,
    pub fields: DraftFields,
    pub generation: u64,
    pub in_flight: Option<SaveSnapshot>,
    /// 已提交，但尚未重新查询到真实时间戳；失败时只能重查，不能重复提交旧版本。
    pub committed: bool,
    pub error: Option<String>,
}

impl EditorSession {
    pub(crate) fn new(detail: TaskDetail) -> Self {
        Self {
            fields: DraftFields::from_task(&detail.task),
            detail,
            generation: 0,
            in_flight: None,
            committed: false,
            error: None,
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.fields != DraftFields::from_task(&self.detail.task)
    }

    pub(crate) fn unfinished(&self) -> bool {
        self.dirty() || self.in_flight.is_some()
    }

    pub(crate) fn edit(&mut self, field: &str, value: String) -> Result<bool, TaskError> {
        if self.detail.context.read_only {
            return Err(TaskError::PlanArchived);
        }
        let previous = self.fields.clone();
        match field {
            "title" => self.fields.title = value,
            "description" => self.fields.description = value,
            "start_date" => {
                if value.is_empty() {
                    self.fields.start_time.clear();
                }
                self.fields.start_date = value;
            }
            "due_date" => {
                if value.is_empty() {
                    self.fields.due_time.clear();
                }
                self.fields.due_date = value;
            }
            _ => return Ok(false),
        }
        if previous == self.fields {
            return Ok(false);
        }
        self.generation = self.generation.saturating_add(1);
        self.error = None;
        Ok(true)
    }

    /// 选择/清除日期时间作为一次输入版本；不能保存日期与时分之间的半成品。
    pub(crate) fn select_datetime(
        &mut self,
        field: &str,
        value: Option<NaiveDateTime>,
    ) -> Result<bool, TaskError> {
        if self.detail.context.read_only {
            return Err(TaskError::PlanArchived);
        }
        let (date, time) = match field {
            "start" => (&mut self.fields.start_date, &mut self.fields.start_time),
            "due" => (&mut self.fields.due_date, &mut self.fields.due_time),
            _ => return Ok(false),
        };
        let next_date = value.map(|at| at.date().to_string()).unwrap_or_default();
        let next_time = value
            .map(|at| at.time().format("%H:%M").to_string())
            .unwrap_or_default();
        if *date == next_date && *time == next_time {
            return Ok(false);
        }
        *date = next_date;
        *time = next_time;
        self.generation = self.generation.saturating_add(1);
        self.error = None;
        Ok(true)
    }

    pub(crate) fn begin_save(&mut self) -> Result<Option<SaveSnapshot>, TaskError> {
        if self.in_flight.is_some() || !self.dirty() {
            return Ok(None);
        }
        if self.detail.context.read_only {
            return Err(TaskError::PlanArchived);
        }
        let snapshot = SaveSnapshot {
            generation: self.generation,
            patch: self.fields.patch(&self.detail.task)?,
            expected_updated_at: self.detail.task.updated_at,
        };
        self.in_flight = Some(snapshot.clone());
        self.committed = false;
        self.error = None;
        Ok(Some(snapshot))
    }

    pub(crate) fn failed(&mut self, message: String) {
        self.in_flight = None;
        self.committed = false;
        self.error = Some(message);
    }

    pub(crate) fn acknowledge(&mut self, detail: TaskDetail) -> bool {
        if detail.task.id != self.detail.task.id || !self.committed {
            return false;
        }
        let Some(snapshot) = self.in_flight.take() else {
            return false;
        };
        if snapshot.generation == self.generation {
            self.fields = DraftFields::from_task(&detail.task);
        }
        self.detail = detail;
        self.committed = false;
        self.error = None;
        true
    }

    pub(crate) fn refresh(&mut self, detail: TaskDetail) {
        if detail.task.id != self.detail.task.id || self.in_flight.is_some() {
            return;
        }
        if self.dirty() {
            // 仅刷新关联标记，保留原基线以便服务检测外部冲突；不暗中覆盖草稿。
            self.detail.context = detail.context;
            self.detail.is_overdue = detail.is_overdue;
            self.detail.is_blocked = detail.is_blocked;
        } else {
            self.fields = DraftFields::from_task(&detail.task);
            self.detail = detail;
        }
    }
}

#[cfg(test)]
mod tests;
