use std::rc::Rc;

use fastnote_core::{
    AppEvent, CommandResult, QueryResult, TaskError, TaskScope, TaskStatus, TodaySectionKind,
};
use fastnote_store::TaskStoreError;
use fastnote_ui::{DialogKind, DialogRequest, ToastKind};

use crate::desktop::runtime::Runtime;
use crate::desktop::task_preferences::hidden_task_created;
use crate::tasks::{DetailPurpose, EditorSession, PendingTask};

use super::view::error_message;

impl Runtime {
    pub(in crate::desktop) fn task_executed(
        self: &Rc<Self>,
        id: u64,
        result: Result<CommandResult, TaskStoreError>,
    ) {
        if self.finished.get() {
            return;
        }
        let pending = self.tasks.borrow_mut().pending.remove(&id);
        let Some(pending) = pending else {
            return;
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let message = error_message(&error);
                match pending {
                    PendingTask::Save { .. } => {
                        if let Some(editor) = self.tasks.borrow_mut().editor.as_mut() {
                            editor.failed(message.clone());
                        }
                        self.task_feedback_timer.stop();
                    }
                    PendingTask::Created { .. } => self
                        .shell
                        .component()
                        .set_task_create_error(message.clone().into()),
                    _ => {}
                }
                self.notice(ToastKind::Error, &message);
                self.sync_task_editor();
                let transition = self.tasks.borrow_mut().after_save.take();
                self.failed_task_transition(transition);
                return;
            }
        };
        if result.needs_confirmation {
            if let PendingTask::Status { id, status } = pending {
                self.tasks.borrow_mut().status_confirmation = Some((id, status));
                self.shell.show_dialog(DialogRequest::new("仍要修改任务状态？",
                    "这项任务仍有未完成的子任务或前置任务。系统只提示，不会锁定操作；确认后继续修改状态。",
                    "继续修改", DialogKind::Normal).with_command(format!("task:status-confirm:{id}")));
            }
            self.sync_task_editor();
            // 确认框打开时不执行延迟的退出/路由，避免吞掉用户决策。
            self.tasks.borrow_mut().after_save = None;
            return;
        }
        let focus_quick = matches!(&pending, PendingTask::Created { .. });
        match pending {
            PendingTask::Save { id } => {
                let selected = {
                    let mut tasks = self.tasks.borrow_mut();
                    if let Some(editor) = tasks
                        .editor
                        .as_mut()
                        .filter(|editor| editor.detail.task.id == id)
                    {
                        editor.committed = true;
                        true
                    } else {
                        false
                    }
                };
                if selected && !self.request_task_detail(DetailPurpose::SaveAck(id)) {
                    let transition = self.tasks.borrow_mut().after_save.take();
                    self.failed_task_transition(transition);
                }
            }
            PendingTask::Created { title } => {
                if self.shell.component().get_task_quick_title() == title {
                    self.shell.component().set_task_quick_title("".into());
                }
                let preferences = self.settings.borrow().today_preferences;
                if preferences.show_unscheduled {
                    self.tasks.borrow_mut().collapsed[TodaySectionKind::Unscheduled as usize] =
                        Some(false);
                }
                if let Some(request) = hidden_task_created(preferences) {
                    self.shell.show_toast(request);
                }
            }
            PendingTask::Status { status, .. } => {
                if !result.affected_tasks.is_empty() {
                    self.notice(
                        ToastKind::Success,
                        match status {
                            TaskStatus::Done => "已完成。",
                            TaskStatus::Todo => "已设为待处理。",
                            TaskStatus::InProgress => "已设为进行中。",
                            TaskStatus::Waiting => "已设为等待中。",
                        },
                    );
                }
            }
            PendingTask::Deleted => {
                let selected_deleted =
                    self.tasks.borrow().editor.as_ref().is_some_and(|editor| {
                        result.affected_tasks.contains(&editor.detail.task.id)
                    });
                if selected_deleted {
                    self.close_task_editor();
                }
                self.notice(ToastKind::Success, "任务已删除。");
            }
            PendingTask::Priority => {}
        }
        self.consume_task_events(&result.events);
        self.sync_task_editor();
        if focus_quick {
            self.shell.component().invoke_focus_task_create();
        }
        self.continue_task_transition();
    }

    fn consume_task_events(&self, events: &[AppEvent]) {
        if events.iter().any(|event| {
            matches!(
                event,
                AppEvent::TasksChanged {
                    scope: TaskScope::Today,
                    ..
                }
            )
        }) {
            self.request_today();
        }
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .filter(|editor| editor.in_flight.is_none())
            .map(|editor| editor.detail.task.id);
        if let Some(id) = selected.filter(|id| events.iter().any(|event|
            matches!(event, AppEvent::TasksChanged { scope: TaskScope::All, .. })
                || matches!(event, AppEvent::TasksChanged { scope: TaskScope::Detail(changed), .. } if changed == id))) {
            self.request_task_detail(DetailPurpose::Refresh(id));
        }
        // Calendar/Plan/Reminder/Note 事件保留在统一命令结果中；其业务页面尚未接入，不能伪称已刷新。
    }

    pub(in crate::desktop) fn task_queried(
        self: &Rc<Self>,
        id: u64,
        result: Result<QueryResult, TaskStoreError>,
    ) {
        if self.finished.get() {
            return;
        }
        let purpose = self.tasks.borrow_mut().queries.remove(&id);
        let Some(purpose) = purpose else {
            if self.tasks.borrow().latest_today != id {
                return;
            }
            match result {
                Ok(QueryResult::Today(view)) => {
                    self.tasks.borrow_mut().today = Some(view);
                    self.shell.component().set_today_state(1);
                    self.shell.component().set_today_error("".into());
                    self.shell.component().set_today_error_log_id("".into());
                    self.apply_today();
                    if self.shell.component().get_current_page() == "今日任务" {
                        let reveal = self.tasks.borrow_mut().take_unscheduled_reveal();
                        if reveal {
                            self.shell
                                .component()
                                .invoke_reveal_task_section(TodaySectionKind::Unscheduled as i32);
                        }
                    }
                }
                Err(error) => {
                    self.shell.component().set_today_state(2);
                    self.shell
                        .component()
                        .set_today_error(error_message(&error).into());
                    self.shell.component().set_today_error_log_id(
                        error
                            .storage_error()
                            .map_or("", |error| error.log_id())
                            .into(),
                    );
                }
                _ => {}
            }
            return;
        };
        if matches!(purpose, DetailPurpose::Open(_)) && self.tasks.borrow().latest_open != id {
            return;
        }
        if matches!(purpose, DetailPurpose::Delete(_)) && self.tasks.borrow().latest_delete != id {
            return;
        }
        let detail = match result {
            Ok(QueryResult::TaskDetail(detail)) => *detail,
            Err(error) => {
                self.task_detail_failed(purpose, error);
                return;
            }
            _ => {
                self.task_detail_failed(purpose, TaskError::Conflict.into());
                return;
            }
        };
        match purpose {
            DetailPurpose::Open(_) => {
                if self.closing.get() {
                    return;
                }
                self.tasks.borrow_mut().editor = Some(EditorSession::new(detail));
                self.task_syncing.set(true);
                self.shell.component().set_task_detail_loading(false);
                self.shell.component().set_task_detail_visible(true);
                self.task_syncing.set(false);
                self.sync_task_editor();
                self.defer_task_focus();
            }
            DetailPurpose::Refresh(id) => {
                if let Some(editor) = self
                    .tasks
                    .borrow_mut()
                    .editor
                    .as_mut()
                    .filter(|editor| editor.detail.task.id == id)
                {
                    editor.refresh(detail);
                }
                self.sync_task_editor();
                self.continue_task_transition();
            }
            DetailPurpose::SaveAck(id) => {
                if let Some(editor) = self
                    .tasks
                    .borrow_mut()
                    .editor
                    .as_mut()
                    .filter(|editor| editor.detail.task.id == id)
                {
                    editor.acknowledge(detail);
                }
                self.task_feedback_timer.stop();
                self.task_saving_visible.set(false);
                self.sync_task_editor();
                self.continue_task_transition();
                let dirty = self
                    .tasks
                    .borrow()
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.dirty() && editor.in_flight.is_none());
                if dirty && self.tasks.borrow().after_save.is_none() {
                    self.schedule_task_save();
                }
            }
            DetailPurpose::Delete(id) => {
                self.tasks.borrow_mut().delete_confirmation = Some(id);
                let count = detail.context.descendant_count.saturating_add(1);
                let (title, body) = if count == 1 {
                    (
                        "删除任务？".to_owned(),
                        "这项任务将被永久删除，无法撤销。".to_owned(),
                    )
                } else {
                    (
                        format!("删除 {count} 个任务？"),
                        format!("将永久删除此任务及其 {} 个子任务，无法撤销。", count - 1),
                    )
                };
                self.shell.show_dialog(
                    DialogRequest::new(title, body, "永久删除", DialogKind::Destructive)
                        .with_command(format!("task:delete-confirm:{id}")),
                );
                self.sync_task_editor();
            }
        }
    }

    fn task_detail_failed(self: &Rc<Self>, purpose: DetailPurpose, error: TaskStoreError) {
        let message = error_message(&error);
        match purpose {
            DetailPurpose::SaveAck(id) => {
                if let Some(editor) = self
                    .tasks
                    .borrow_mut()
                    .editor
                    .as_mut()
                    .filter(|editor| editor.detail.task.id == id)
                {
                    editor.error = Some(message.clone());
                }
                let transition = self.tasks.borrow_mut().after_save.take();
                self.failed_task_transition(transition);
            }
            DetailPurpose::Open(_) => {
                self.task_syncing.set(true);
                self.shell.component().set_task_detail_loading(false);
                self.shell
                    .component()
                    .set_task_detail_visible(self.tasks.borrow().editor.is_some());
                self.task_syncing.set(false);
            }
            DetailPurpose::Refresh(id) => {
                let clean_deleted =
                    error.domain_error() == Some(TaskError::NotFound)
                        && self.tasks.borrow().editor.as_ref().is_some_and(|editor| {
                            editor.detail.task.id == id && !editor.unfinished()
                        });
                if clean_deleted {
                    self.close_task_editor();
                } else if let Some(editor) = self
                    .tasks
                    .borrow_mut()
                    .editor
                    .as_mut()
                    .filter(|editor| editor.detail.task.id == id)
                {
                    editor.error = Some(message.clone());
                }
            }
            DetailPurpose::Delete(_) => {}
        }
        self.notice(ToastKind::Error, &message);
        self.sync_task_editor();
    }
}
