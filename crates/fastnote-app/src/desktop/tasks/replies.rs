mod failures;
mod state;
mod today;

use std::rc::Rc;

use fastnote_core::{
    AppEvent, CommandResult, QueryResult, TaskError, TaskStatus, TodaySectionKind,
};
use fastnote_store::TaskStoreError;
use fastnote_ui::{DialogKind, DialogRequest, ToastKind};

use crate::desktop::effects::{FocusTarget, UiEffect};
use crate::desktop::runtime::Runtime;
use crate::desktop::task_preferences::hidden_task_created;
use crate::tasks::{DetailPurpose, EditorSession, PendingTask};

use super::view::error_message;
use state::{
    acknowledge_save, invalidations, mark_save_committed, mark_save_failed, may_apply_detail,
    should_refresh, stage_confirmation,
};

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
                    PendingTask::Save { id } => {
                        let changed =
                            mark_save_failed(&mut self.tasks.borrow_mut(), id, message.clone());
                        if changed {
                            self.detail_dirty.set(true);
                            let _ = self.with_ui(|ui| ui.task_feedback_timer.stop());
                        }
                    }
                    PendingTask::Created { .. } => {
                        let _ = self.with_ui(|ui| {
                            ui.shell
                                .component()
                                .set_task_create_error(message.clone().into());
                        });
                    }
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
            // 确认未写入；先撤销延迟转场，再登记待确认状态，不能在投影回调中启动转场。
            self.tasks.borrow_mut().after_save = None;
            if let PendingTask::Status { id, status } = pending {
                let accepted = stage_confirmation(
                    || {
                        self.tasks
                            .borrow_mut()
                            .status_confirmation
                            .replace((id, status))
                    },
                    || {
                        self.emit_ui(UiEffect::Dialog(DialogRequest::new("仍要修改任务状态？",
                        "这项任务仍有未完成的子任务或前置任务。系统只提示，不会锁定操作；确认后继续修改状态。",
                        "继续修改", DialogKind::Normal).with_command(format!("task:status-confirm:{id}"))))
                    },
                    |previous| self.tasks.borrow_mut().status_confirmation = previous,
                );
                if !accepted {
                    self.notice(ToastKind::Error, "待确认操作过多，请先处理当前确认后重试。");
                }
            }
            self.sync_task_editor();
            return;
        }
        self.mark_task_invalidations(&result.events);
        let focus_quick = matches!(&pending, PendingTask::Created { .. });
        match pending {
            PendingTask::Save { id } => {
                let selected = mark_save_committed(&mut self.tasks.borrow_mut(), id);
                // R3：保留既有保存基线确认读取；它不是无窗页面预取，不能因无 UI 丢确认。
                if selected && !self.request_task_detail(DetailPurpose::SaveAck(id)) {
                    let transition = self.tasks.borrow_mut().after_save.take();
                    self.failed_task_transition(transition);
                }
            }
            PendingTask::Created { title } => {
                let preferences = self.settings.borrow().today_preferences;
                if preferences.show_unscheduled {
                    self.tasks.borrow_mut().collapsed[TodaySectionKind::Unscheduled as usize] =
                        Some(false);
                }
                let _ = self.with_ui(|ui| {
                    if ui.shell.component().get_task_quick_title() == title {
                        ui.shell.component().set_task_quick_title("".into());
                    }
                });
                if let Some(request) = hidden_task_created(preferences) {
                    self.emit_ui(UiEffect::Toast(request));
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
            self.emit_ui(UiEffect::Focus(FocusTarget::TaskCreate));
        }
        self.continue_task_transition();
    }

    fn mark_task_invalidations(&self, events: &[AppEvent]) {
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .map(|editor| editor.detail.task.id);
        let changed = invalidations(events, selected);
        if changed.today {
            self.today_dirty.set(true);
            self.tasks.borrow_mut().invalidate_today_read();
        }
        if changed.detail {
            self.detail_dirty.set(true);
        }
    }

    fn consume_task_events(&self, events: &[AppEvent]) {
        let page_visible = self.today_page_visible();
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .filter(|editor| editor.in_flight.is_none())
            .map(|editor| editor.detail.task.id);
        let changed = invalidations(events, selected);
        if should_refresh(changed.today, page_visible) {
            self.request_today();
        }
        if should_refresh(changed.detail, page_visible) {
            if let Some(id) = selected {
                self.request_task_detail(DetailPurpose::Refresh(id));
            }
        }
        // Calendar/Plan/Reminder/Note 事件保留在统一命令结果中；其业务页面尚未接入，不能伪称已刷新。
    }

    pub(in crate::desktop) fn task_queried(
        self: &Rc<Self>,
        id: u64,
        result: Result<QueryResult, TaskStoreError>,
    ) {
        if self.finished.get() || id == 0 {
            return;
        }
        let purpose = self.tasks.borrow_mut().queries.remove(&id);
        let Some(purpose) = purpose else {
            self.today_queried(id, result);
            return;
        };
        if matches!(purpose, DetailPurpose::Open(_)) && self.tasks.borrow().latest_open != id {
            return;
        }
        if matches!(purpose, DetailPurpose::Delete(_)) && self.tasks.borrow().latest_delete != id {
            return;
        }
        if matches!(purpose, DetailPurpose::Open(_)) {
            self.tasks.borrow_mut().latest_open = 0;
        }
        let page_visible = self.today_page_visible();
        if !may_apply_detail(purpose, page_visible) {
            self.detail_dirty.set(true);
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
                self.detail_dirty.set(false);
                self.task_syncing.set(true);
                let _ = self.with_ui(|ui| {
                    ui.shell.component().set_task_detail_loading(false);
                    ui.shell.component().set_task_detail_visible(true);
                });
                self.task_syncing.set(false);
                self.sync_task_editor();
                self.defer_task_focus();
            }
            DetailPurpose::Refresh(id) => {
                let mut tasks = self.tasks.borrow_mut();
                let Some(editor) = tasks
                    .editor
                    .as_mut()
                    .filter(|editor| editor.detail.task.id == id)
                else {
                    return;
                };
                editor.refresh(detail);
                drop(tasks);
                self.detail_dirty.set(false);
                self.sync_task_editor();
                self.continue_task_transition();
            }
            DetailPurpose::SaveAck(id) => {
                if acknowledge_save(&mut self.tasks.borrow_mut(), id, detail) {
                    self.detail_dirty.set(false);
                }
                let _ = self.with_ui(|ui| {
                    ui.task_feedback_timer.stop();
                    ui.task_saving_visible.set(false);
                });
                self.sync_task_editor();
                self.continue_task_transition();
                let dirty = self
                    .tasks
                    .borrow()
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.dirty() && editor.in_flight.is_none());
                if dirty && self.tasks.borrow().after_save.is_none() {
                    if self.today_page_visible() {
                        self.schedule_task_save();
                    } else {
                        self.flush_task_editor();
                    }
                }
            }
            DetailPurpose::Delete(id) => {
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
                let accepted = stage_confirmation(
                    || self.tasks.borrow_mut().delete_confirmation.replace(id),
                    || {
                        self.emit_ui(UiEffect::Dialog(
                            DialogRequest::new(title, body, "永久删除", DialogKind::Destructive)
                                .with_command(format!("task:delete-confirm:{id}")),
                        ))
                    },
                    |previous| self.tasks.borrow_mut().delete_confirmation = previous,
                );
                if !accepted {
                    self.notice(ToastKind::Error, "待确认操作过多，请先处理当前确认后重试。");
                }
                self.sync_task_editor();
            }
        }
    }
}

#[cfg(test)]
mod contract;
