use std::rc::Rc;

use fastnote_core::TaskError;
use fastnote_store::TaskStoreError;
use fastnote_ui::ToastKind;

use crate::desktop::runtime::Runtime;
use crate::tasks::DetailPurpose;

use super::super::view::error_message;

impl Runtime {
    pub(super) fn task_detail_failed(
        self: &Rc<Self>,
        purpose: DetailPurpose,
        error: TaskStoreError,
    ) {
        self.detail_dirty.set(true);
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
                let _ = self.with_ui(|ui| {
                    ui.shell.component().set_task_detail_loading(false);
                    ui.shell
                        .component()
                        .set_task_detail_visible(self.tasks.borrow().editor.is_some());
                });
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
