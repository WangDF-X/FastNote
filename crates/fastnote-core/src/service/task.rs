use crate::derive::{calendar::build_day_agenda, derive_task_summary, TodayCollector};
use crate::{
    AppEvent, Command, CommandResult, PlanId, Query, QueryResult, ReminderOwner, Task, TaskClock,
    TaskDetail, TaskError, TaskId, TaskRepository, TaskScope, TaskStatus, TaskSummary, WarningCode,
};

/// 统一任务服务；不持有 UI、平台或数据库库类型。
pub struct TaskService<'a, R: TaskRepository, C: TaskClock> {
    repository: &'a mut R,
    clock: C,
}

impl<'a, R: TaskRepository, C: TaskClock> TaskService<'a, R, C> {
    /// 使用 DB 线程内的仓储和应用注入的时钟装配。
    pub fn new(repository: &'a mut R, clock: C) -> Self {
        Self { repository, clock }
    }

    /// 校验、执行并仅在提交后返回变更事件。
    pub fn execute(&mut self, command: Command) -> Result<CommandResult, R::Error> {
        match command {
            Command::CreateTask { id, title } => {
                let task = Task::new(id, title, self.clock.now())?;
                self.repository.insert_task(&task)?;
                Ok(changed(vec![id], None, false))
            }
            Command::CreateCalendarTask { id, title, date } => {
                let mut task = Task::new(id, title, self.clock.now())?;
                task.start_date = Some(date);
                task.validate()?;
                self.repository.insert_task(&task)?;
                Ok(changed(vec![id], None, false))
            }
            Command::UpdateTask {
                id,
                patch,
                expected_updated_at,
            } => {
                let detail = self.editable(id)?;
                if detail.task.updated_at != expected_updated_at {
                    return Err(TaskError::Conflict.into());
                }
                let next = detail.task.patched(&patch, self.clock.now())?;
                if next == detail.task {
                    return Ok(CommandResult::default());
                }
                self.repository.replace_task(&detail.task, &next)?;
                Ok(changed(vec![id], next.plan_id, false))
            }
            Command::SetTaskStatus {
                id,
                status,
                confirmed,
            } => {
                let detail = self.editable(id)?;
                let next = detail.task.with_status(status, self.clock.now())?;
                if next == detail.task {
                    return Ok(CommandResult::default());
                }
                let mut warnings = Vec::new();
                if status == TaskStatus::Done
                    && detail.context.child_done < detail.context.child_total
                {
                    warnings.push(WarningCode::ParentIncomplete);
                }
                if matches!(status, TaskStatus::InProgress | TaskStatus::Done) && detail.is_blocked
                {
                    warnings.push(WarningCode::DependencyIncomplete);
                }
                if !confirmed && !warnings.is_empty() {
                    return Ok(CommandResult {
                        warnings,
                        needs_confirmation: true,
                        ..Default::default()
                    });
                }
                self.repository.replace_task(&detail.task, &next)?;
                Ok(changed(vec![id], next.plan_id, status == TaskStatus::Done))
            }
            Command::DeleteTask { id, cascade } => {
                let detail = self.editable(id)?;
                let affected = self.repository.delete_task(id, cascade)?;
                let mut result = changed(affected, detail.task.plan_id, true);
                result.events.push(AppEvent::NotesChanged);
                Ok(result)
            }
        }
    }

    /// 统一今日分组/排序或详情标记；不存在返回稳定领域错误。
    pub fn query(&self, query: Query) -> Result<QueryResult, R::Error> {
        match query {
            Query::Calendar { start, end } => {
                let days = (end - start).num_days();
                if !(0..62).contains(&days) {
                    return Err(TaskError::Conflict.into());
                }
                let tasks = self.repository.calendar_tasks(start, end)?;
                let mut result = Vec::with_capacity(days as usize + 1);
                for offset in 0..=days {
                    let Some(date) = start.checked_add_signed(chrono::Duration::days(offset))
                    else {
                        return Err(TaskError::Conflict.into());
                    };
                    result.push((
                        date,
                        build_day_agenda(date, tasks.iter().cloned(), &self.clock),
                    ));
                }
                Ok(QueryResult::Calendar(result))
            }
            Query::Today { date, limits } => {
                let mut collector = TodayCollector::new(
                    date,
                    &self.clock,
                    self.repository.today_preferences()?,
                    limits,
                );
                self.repository
                    .visit_today_tasks(date, &mut |task| collector.push(task))?;
                let mut view = collector.finish();
                self.repository.complete_today_projection(&mut view)?;
                for section in &mut view.sections {
                    for task in &mut section.items {
                        task.is_blocked = task.context.blocking_count > 0;
                    }
                }
                Ok(QueryResult::Today(view))
            }
            Query::TaskDetail(id) => Ok(QueryResult::TaskDetail(Box::new(self.detail(id)?))),
        }
    }

    fn detail(&self, id: TaskId) -> Result<TaskDetail, R::Error> {
        let mut detail = self
            .repository
            .task_detail(id)?
            .ok_or(TaskError::NotFound)?;
        let mut summary = TaskSummary::from_task(&detail.task, detail.context.clone());
        derive_task_summary(&mut summary, self.clock.today(), &self.clock);
        detail.is_overdue = summary.is_overdue;
        detail.is_blocked = summary.is_blocked;
        Ok(detail)
    }

    fn editable(&self, id: TaskId) -> Result<TaskDetail, R::Error> {
        let detail = self.detail(id)?;
        if detail.context.read_only {
            return Err(TaskError::PlanArchived.into());
        }
        Ok(detail)
    }
}

fn changed(ids: Vec<TaskId>, plan: Option<PlanId>, reminders: bool) -> CommandResult {
    let mut scopes = vec![TaskScope::Today, TaskScope::Calendar, TaskScope::All];
    if let Some(id) = ids.first() {
        scopes.push(TaskScope::Detail(*id));
    }
    if let Some(plan) = plan {
        scopes.push(TaskScope::Plan(plan));
    }
    let mut events: Vec<_> = scopes
        .into_iter()
        .map(|scope| AppEvent::TasksChanged {
            scope,
            affected: ids.clone(),
        })
        .collect();
    if reminders {
        events.push(AppEvent::RemindersChanged {
            owners: ids.iter().copied().map(ReminderOwner::Task).collect(),
        });
    }
    CommandResult {
        affected_tasks: ids,
        affected_plans: plan.into_iter().collect(),
        events,
        ..Default::default()
    }
}
