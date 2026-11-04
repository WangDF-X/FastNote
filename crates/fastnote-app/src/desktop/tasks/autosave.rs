use std::rc::Rc;
use std::time::Duration;

use fastnote_core::Command;
use fastnote_ui::{task_error_message, ToastKind};
use slint::TimerMode;

use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, PendingTask};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::desktop) enum FlushStatus {
    Clean,
    Pending,
    Blocked,
}

impl Runtime {
    pub(in crate::desktop) fn edit_task_field(self: &Rc<Self>, field: &str, value: String) {
        if self.task_syncing.get() || !self.can_task_interact() || !self.can_focus_tasks() {
            return;
        }
        let result = self
            .tasks
            .borrow_mut()
            .editor
            .as_mut()
            .map(|editor| editor.edit(field, value));
        match result {
            Some(Ok(true)) => {
                self.sync_task_editor();
                match field {
                    "priority" => {
                        self.flush_task_editor();
                    }
                    "title" | "description" => self.schedule_task_save(),
                    _ => {}
                }
            }
            Some(Err(error)) => self.notice(ToastKind::Error, task_error_message(error)),
            _ => {}
        }
    }

    pub(in crate::desktop) fn schedule_task_save(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let _ = self.with_ui(|ui| {
            ui.task_save_timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(500),
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.flush_task_editor();
                    }
                },
            );
        });
    }

    pub(in crate::desktop) fn flush_task_editor(self: &Rc<Self>) -> FlushStatus {
        let _ = self.with_ui(|ui| ui.task_save_timer.stop());
        if self.task_syncing.get() || self.finished.get() {
            return FlushStatus::Blocked;
        }
        let result = {
            let mut tasks = self.tasks.borrow_mut();
            let Some(editor) = tasks.editor.as_mut() else {
                return FlushStatus::Clean;
            };
            if editor.in_flight.is_some() {
                return FlushStatus::Pending;
            }
            editor
                .begin_save()
                .map(|snapshot| snapshot.map(|snapshot| (editor.detail.task.id, snapshot)))
        };
        match result {
            Ok(None) => FlushStatus::Clean,
            Ok(Some((id, snapshot))) => {
                if !self.queue_task_command(
                    Command::UpdateTask {
                        id,
                        patch: snapshot.patch,
                        expected_updated_at: snapshot.expected_updated_at,
                    },
                    PendingTask::Save { id },
                ) {
                    if let Some(editor) = self.tasks.borrow_mut().editor.as_mut() {
                        editor.failed("任务保存队列不可用，输入已保留，请重试。".to_owned());
                    }
                    self.sync_task_editor();
                    return FlushStatus::Blocked;
                }
                let weak = Rc::downgrade(self);
                let _ = self.with_ui(|ui| {
                    ui.task_saving_visible.set(false);
                    ui.task_feedback_timer.start(
                        TimerMode::SingleShot,
                        Duration::from_millis(300),
                        move || {
                            if let Some(runtime) = weak.upgrade() {
                                let _ = runtime.with_ui(|ui| ui.task_saving_visible.set(true));
                                runtime.sync_task_editor();
                            }
                        },
                    );
                });
                FlushStatus::Pending
            }
            Err(error) => {
                let message = task_error_message(error);
                if let Some(editor) = self.tasks.borrow_mut().editor.as_mut() {
                    editor.error = Some(message.to_owned());
                }
                self.notice(ToastKind::Error, message);
                self.sync_task_editor();
                FlushStatus::Blocked
            }
        }
    }

    pub(in crate::desktop) fn retry_task_save(self: &Rc<Self>) {
        let committed = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .filter(|editor| editor.committed)
            .map(|editor| editor.detail.task.id);
        if let Some(id) = committed {
            self.request_task_detail(DetailPurpose::SaveAck(id));
        } else {
            self.flush_task_editor();
        }
    }
}
