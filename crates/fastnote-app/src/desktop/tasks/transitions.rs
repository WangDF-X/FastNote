mod confirmations;

use std::rc::Rc;

use fastnote_core::{CloseBehavior, Command};
use fastnote_ui::{DialogKind, DialogRequest};

use crate::desktop::effects::{FocusTarget, UiEffect};
use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, PendingTask, Transition};

use super::autosave::FlushStatus;

impl Runtime {
    pub(in crate::desktop) fn task_transition(self: &Rc<Self>, transition: Transition) {
        if !self.can_task_interact() || !self.can_focus_tasks() {
            return;
        }
        let current_focus = {
            let tasks = self.tasks.borrow();
            tasks
                .current_editor_focus(&transition)
                .zip(tasks.editor.as_ref().map(|editor| editor.detail.task.id))
        };
        if let Some((focus, id)) = current_focus.filter(|_| {
            self.with_ui(|ui| {
                ui.shell.component().get_task_detail_visible()
                    && !ui.shell.component().get_task_detail_loading()
            })
            .unwrap_or(false)
        }) {
            // 同一任务的「设置日期」只是移动焦点，不重查、不丢弃已有/非法草稿。
            let _ = self.with_ui(|ui| {
                ui.shell.component().set_task_detail_focus(focus.into());
            });
            self.emit_ui(UiEffect::Focus(FocusTarget::TaskEditor(id)));
            return;
        }
        if !self.defer_task_transition(transition.clone()) {
            self.perform_task_transition(transition);
        }
    }

    /// 返回 true 表示必须等待保存或用户修正，调用者不能隐藏/丢弃输入。
    pub(in crate::desktop) fn defer_task_transition(
        self: &Rc<Self>,
        transition: Transition,
    ) -> bool {
        if !self.tasks.borrow().unfinished() {
            return false;
        }
        self.tasks.borrow_mut().after_save = Some(transition);
        match self.flush_task_editor() {
            FlushStatus::Blocked => {
                let transition = self.tasks.borrow_mut().after_save.take();
                self.failed_task_transition(transition);
                true
            }
            FlushStatus::Pending => true,
            FlushStatus::Clean => {
                if !self.tasks.borrow().pending.is_empty() {
                    return true;
                }
                self.tasks.borrow_mut().after_save = None;
                false
            }
        }
    }

    pub(in crate::desktop) fn continue_task_transition(self: &Rc<Self>) {
        if self.finished.get() || self.closing.get() || !self.tasks.borrow().pending.is_empty() {
            return;
        }
        let unfinished = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .is_some_and(|editor| editor.unfinished());
        if unfinished {
            let has_transition = self.tasks.borrow().after_save.is_some();
            if has_transition && self.flush_task_editor() == FlushStatus::Blocked {
                let transition = self.tasks.borrow_mut().after_save.take();
                self.failed_task_transition(transition);
            }
            return;
        }
        let transition = self.tasks.borrow_mut().take_ready_transition();
        if let Some(transition) = transition {
            self.perform_task_transition(transition);
        }
    }

    fn perform_task_transition(self: &Rc<Self>, transition: Transition) -> bool {
        let focus = transition.editor_focus();
        match transition {
            Transition::Navigate(action) => {
                self.dispatch_action(action);
                true
            }
            Transition::Open(id) | Transition::OpenDates(id) => {
                let accepted = self.request_task_detail(DetailPurpose::Open(id));
                if accepted {
                    if let Some(focus) = focus {
                        let _ = self
                            .with_ui(|ui| ui.shell.component().set_task_detail_focus(focus.into()));
                    }
                }
                accepted
            }
            Transition::CloseDetail => {
                self.close_task_editor();
                true
            }
            Transition::Close { exit } => {
                self.request_close(exit);
                true
            }
            Transition::Minimize => {
                self.minimize_window();
                true
            }
            Transition::Status {
                id,
                status,
                confirmed,
            } => self.queue_task_command(
                Command::SetTaskStatus {
                    id,
                    status,
                    confirmed,
                },
                PendingTask::Status { id, status },
            ),
            Transition::Delete(id) => self.request_task_detail(DetailPurpose::Delete(id)),
            Transition::DeleteConfirmed(id) => self.queue_task_command(
                Command::DeleteTask { id, cascade: true },
                PendingTask::Deleted,
            ),
            Transition::Priority(id) => {
                self.cycle_task_priority(id);
                true
            }
        }
    }

    pub(in crate::desktop) fn failed_task_transition(
        self: &Rc<Self>,
        transition: Option<Transition>,
    ) {
        let minimize = matches!(transition, Some(Transition::Minimize));
        if minimize && self.with_ui(|_| ()).is_some() {
            self.show_main();
        }
        let exit = matches!(transition, Some(Transition::Close { exit: true }))
            || (matches!(transition, Some(Transition::Close { .. }))
                && (self.settings.borrow().close_behavior == CloseBehavior::Quit
                    || self.entry.get().is_none()));
        if exit {
            let previous =
                std::mem::replace(&mut self.tasks.borrow_mut().discard_exit_allowed, true);
            if !self.emit_ui(UiEffect::Dialog(DialogRequest::new("任务尚未保存，仍然退出？",
                    "当前输入未能完成保存或刷新。仍然退出会丢弃未保存的输入，数据库中已有内容不会被重建或覆盖。",
                    "放弃输入并退出", DialogKind::Destructive).with_command("task:discard-and-quit"))) {
                self.tasks.borrow_mut().discard_exit_allowed = previous;
            }
        }
    }
}
