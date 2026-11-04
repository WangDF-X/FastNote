use std::rc::Rc;

use fastnote_core::TaskId;
use fastnote_ui::{DialogKind, DialogRequest, ToastKind};

use crate::desktop::effects::UiEffect;
use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, Transition};

use super::super::policy::{may_query, resolve_confirmation, stage_confirmation};

impl Runtime {
    /// 重新加载只在用户确认后丢弃输入；无会话仍保留确认，不为提示创建页面。
    pub(in crate::desktop) fn request_editor_reload(self: &Rc<Self>) {
        if !self.task_kernel_available() || self.dialog_blocked() {
            return;
        }
        let selected = self.tasks.borrow().editor.as_ref().map(|editor| {
            (
                editor.detail.task.id,
                editor.unfinished(),
                editor.in_flight.is_some(),
            )
        });
        let Some((id, unfinished, saving)) = selected else {
            return;
        };
        if saving || !self.tasks.borrow().pending.is_empty() {
            self.notice(ToastKind::Info, "正在保存任务，请稍候。");
            return;
        }
        if unfinished {
            let accepted = stage_confirmation(
                || self.tasks.borrow_mut().reload_confirmation.replace(id),
                || {
                    self.emit_ui(UiEffect::Dialog(
                        DialogRequest::new(
                            "重新加载任务？",
                            "未保存的输入会被丢弃，重新读取数据库中的内容。",
                            "重新加载",
                            DialogKind::Destructive,
                        )
                        .with_command(format!("task:reload-confirm:{id}")),
                    ))
                },
                |previous| self.tasks.borrow_mut().reload_confirmation = previous,
            );
            if !accepted {
                self.notice(ToastKind::Error, "待确认操作过多，请先处理当前确认后重试。");
            }
        } else {
            self.request_task_detail(DetailPurpose::Open(id));
        }
    }

    /// 确认命令已由用户决策，不再经过要求 UI 的页面交互入口；失败时保留原许可。
    pub(in crate::desktop) fn handle_task_action(self: &Rc<Self>, action: &str) -> bool {
        if action == "task:discard-and-quit" {
            if self.finished.get() || self.closing.get() || self.busy.get() {
                return true;
            }
            resolve_confirmation(
                || {
                    let mut tasks = self.tasks.borrow_mut();
                    std::mem::take(&mut tasks.discard_exit_allowed).then_some(())
                },
                |_| {
                    self.close_task_editor();
                    self.finish_exit();
                    true
                },
                |_| self.tasks.borrow_mut().discard_exit_allowed = true,
            );
            return true;
        }
        if let Some(value) = action.strip_prefix("task:delete-confirm:") {
            let id = value.parse::<TaskId>().ok();
            if self.confirmed_mutation_ready() {
                resolve_confirmation(
                    || {
                        self.tasks
                            .borrow_mut()
                            .delete_confirmation
                            .take_if(|confirmed| Some(*confirmed) == id)
                    },
                    |id| self.perform_task_transition(Transition::DeleteConfirmed(*id)),
                    |previous| self.tasks.borrow_mut().delete_confirmation = Some(previous),
                );
            }
            return true;
        }
        if let Some(value) = action.strip_prefix("task:status-confirm:") {
            let id = value.parse::<TaskId>().ok();
            if self.confirmed_mutation_ready() {
                resolve_confirmation(
                    || {
                        self.tasks
                            .borrow_mut()
                            .status_confirmation
                            .take_if(|(confirmed, _)| Some(*confirmed) == id)
                    },
                    |(id, status)| {
                        self.perform_task_transition(Transition::Status {
                            id: *id,
                            status: *status,
                            confirmed: true,
                        })
                    },
                    |previous| self.tasks.borrow_mut().status_confirmation = Some(previous),
                );
            }
            return true;
        }
        if let Some(value) = action.strip_prefix("task:reload-confirm:") {
            let id = value.parse::<TaskId>().ok();
            if !self.task_kernel_available() || !self.tasks.borrow().pending.is_empty() {
                return true;
            }
            if !may_query(id.map(DetailPurpose::Open), self.with_ui(|_| ()).is_some()) {
                self.detail_dirty.set(true);
                return true;
            }
            let selected = self.tasks.borrow().editor.as_ref().is_some_and(|editor| {
                Some(editor.detail.task.id) == id && editor.in_flight.is_none()
            });
            if selected {
                resolve_confirmation(
                    || {
                        self.tasks
                            .borrow_mut()
                            .reload_confirmation
                            .take_if(|confirmed| Some(*confirmed) == id)
                    },
                    |id| {
                        // R3：先确认读取已被接受，再按用户许可丢弃输入；失败不能把编辑器清空。
                        self.request_task_detail(DetailPurpose::Open(*id))
                            && self.discard_task_editor_for_reload(*id)
                    },
                    |previous| self.tasks.borrow_mut().reload_confirmation = Some(previous),
                );
            }
            return true;
        }
        false
    }

    fn confirmed_mutation_ready(&self) -> bool {
        self.task_kernel_available() && !self.tasks.borrow().unfinished()
    }
}
