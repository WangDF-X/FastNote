mod loading;
mod timing;

use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{Command, Query, Task, TaskClock, TaskError, TaskId, TaskPatch, TaskStatus};
use fastnote_ui::{task_error_message, ToastKind};
use slint::Timer;
use uuid::Uuid;

use crate::clock::SystemClock;
use crate::desktop::effects::{FocusTarget, UiEffect};
use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, PageScope, PendingTask, Transition};

use super::policy::{detail_query_pending, kernel_available, may_query, save_read_failed};
use super::view::state::while_syncing;

impl Runtime {
    pub(in crate::desktop) fn can_task_interact(&self) -> bool {
        self.task_kernel_available() && self.today_page_visible() && !self.dialog_blocked()
    }

    /// 已确认的业务不要求主窗存在；页面输入另经 can_task_interact 检查。
    pub(in crate::desktop) fn task_kernel_available(&self) -> bool {
        kernel_available(
            self.ready.get(),
            self.finished.get(),
            self.closing.get(),
            self.busy.get(),
        )
    }

    pub(in crate::desktop) fn queue_task_command(
        &self,
        command: Command,
        pending: PendingTask,
    ) -> bool {
        if !self.ready.get() || self.finished.get() || self.closing.get() {
            return false;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        let source = if matches!(pending, PendingTask::Save { .. }) {
            "detail"
        } else {
            "today"
        };
        if let Err(error) = self.worker.execute(id, command, source) {
            tracing::warn!(request_id = id, error_kind = ?error, "task queue unavailable");
            self.notice(
                ToastKind::Error,
                "任务保存队列不可用，请稍后重试。输入已保留。",
            );
            return false;
        }
        self.tasks.borrow_mut().pending.insert(id, pending);
        self.sync_task_editor();
        true
    }

    pub(in crate::desktop) fn request_today(&self) {
        if self.finished.get() {
            return;
        }
        self.today_dirty.set(true);
        let pending = {
            let mut tasks = self.tasks.borrow_mut();
            if tasks.latest_today != 0 {
                tasks.invalidate_today_read();
                true
            } else {
                false
            }
        };
        if !self.ready.get()
            || self.closing.get()
            || !may_query(None, self.today_page_visible())
            || pending
        {
            return;
        }
        let Some(stamp) = self.ui_epoch.get().stamp(PageScope::Today) else {
            return;
        };
        let date = self.observe_task_clock().sample.local.date();
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        let limits = self.tasks.borrow().limits;
        if self
            .worker
            .query(id, Query::Today { date, limits })
            .is_err()
        {
            let _ = self.with_ui(|ui| {
                ui.shell.component().set_today_state(2);
                ui.shell
                    .component()
                    .set_today_error("查询队列不可用，请稍后重试。".into());
                ui.shell.component().set_today_error_log_id("".into());
            });
            return;
        }
        {
            let mut tasks = self.tasks.borrow_mut();
            tasks.latest_today = id;
            tasks.today_requery = false;
            tasks.read_stamps.insert(id, stamp);
        }
        tracing::info!(
            phase = "page_query_queued",
            request_id = id,
            query_scope = "today",
            session_generation = stamp.session_generation,
            page_generation = stamp.page_generation
        );
        if self.tasks.borrow().today.is_none() {
            let _ = self.with_ui(|ui| ui.shell.component().set_today_state(0));
        }
    }

    pub(in crate::desktop) fn request_task_detail(&self, purpose: DetailPurpose) -> bool {
        if self.finished.get() {
            return false;
        }
        self.detail_dirty.set(true);
        if !self.ready.get() || !may_query(Some(purpose), self.today_page_visible()) {
            return false;
        }
        if self.closing.get()
            && !matches!(
                purpose,
                DetailPurpose::SaveAck(_) | DetailPurpose::Delete(_)
            )
        {
            return false;
        }
        if let DetailPurpose::Refresh(id) = purpose {
            if !self
                .tasks
                .borrow()
                .editor
                .as_ref()
                .is_some_and(|editor| editor.detail.task.id == id && editor.in_flight.is_none())
            {
                return false;
            }
        }
        let task_id = match purpose {
            DetailPurpose::Open(id)
            | DetailPurpose::Refresh(id)
            | DetailPurpose::SaveAck(id)
            | DetailPurpose::Delete(id) => id,
        };
        let stamp = match purpose {
            DetailPurpose::Open(_) | DetailPurpose::Refresh(_) => {
                let Some(stamp) = self.ui_epoch.get().stamp(PageScope::Today) else {
                    return false;
                };
                Some(stamp)
            }
            DetailPurpose::SaveAck(_) | DetailPurpose::Delete(_) => None,
        };
        if detail_query_pending(&self.tasks.borrow(), purpose) {
            return true;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self.worker.query(id, Query::TaskDetail(task_id)).is_err() {
            let message = "任务查询队列不可用，请稍后重试。";
            save_read_failed(&mut self.tasks.borrow_mut(), purpose, message);
            self.notice(ToastKind::Error, message);
            self.sync_task_editor();
            return false;
        }
        {
            let mut tasks = self.tasks.borrow_mut();
            tasks.queries.insert(id, purpose);
            if let Some(stamp) = stamp {
                tasks.read_stamps.insert(id, stamp);
            }
            match purpose {
                DetailPurpose::Open(task_id) => {
                    tasks.latest_open = id;
                    if tasks.resume.selected != Some(task_id) {
                        tasks.resume.focus_pending = false;
                    }
                    tasks.resume.selected = Some(task_id);
                }
                DetailPurpose::Delete(_) => tasks.latest_delete = id,
                _ => {}
            }
        }
        tracing::info!(
            phase = "detail_query_queued",
            request_id = id,
            query_scope = match purpose {
                DetailPurpose::Open(_) => "task_open",
                DetailPurpose::Refresh(_) => "task_refresh",
                DetailPurpose::SaveAck(_) => "task_save_ack",
                DetailPurpose::Delete(_) => "task_delete_confirmation",
            },
            session_generation = stamp.map_or(0, |stamp| stamp.session_generation),
            page_generation = stamp.map_or(0, |stamp| stamp.page_generation)
        );
        if matches!(purpose, DetailPurpose::Open(_)) {
            let _ = self.with_ui(|ui| {
                while_syncing(&self.task_syncing, || {
                    ui.shell.component().set_task_detail_visible(true);
                    ui.shell.component().set_task_detail_loading(true);
                });
            });
        }
        self.sync_task_editor();
        true
    }

    pub(in crate::desktop) fn create_task(self: &Rc<Self>, title: String) {
        if !self.can_task_interact() || !self.can_focus_tasks() {
            return;
        }
        if self.tasks.borrow().unfinished() {
            self.notice(
                ToastKind::Info,
                "请先修正并保存当前任务，再添加新任务。输入已保留。",
            );
            return;
        }
        let id = match TaskId::try_from(Uuid::now_v7()) {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(error_kind = ?error, "generated task ID invalid");
                self.notice(ToastKind::Error, "无法创建任务标识，请重试。");
                return;
            }
        };
        if let Err(error) = Task::new(id, title.clone(), SystemClock.now()) {
            let _ = self.with_ui(|ui| {
                ui.shell
                    .component()
                    .set_task_create_error(task_error_message(error).into());
            });
            return;
        }
        let _ = self.with_ui(|ui| ui.shell.component().set_task_create_error("".into()));
        self.queue_task_command(
            Command::CreateTask {
                id,
                title: title.clone(),
            },
            PendingTask::Created { title },
        );
    }

    pub(in crate::desktop) fn toggle_task(self: &Rc<Self>, id: TaskId) {
        let status = {
            let tasks = self.tasks.borrow();
            tasks
                .editor
                .as_ref()
                .filter(|editor| editor.detail.task.id == id)
                .and_then(|editor| editor.detail.task.status)
                .or_else(|| {
                    tasks.today.as_ref().and_then(|view| {
                        view.sections
                            .iter()
                            .flat_map(|section| &section.items)
                            .find(|task| task.id == id)
                            .and_then(|task| task.status)
                    })
                })
        };
        if let Some(status) = status {
            self.task_transition(Transition::Status {
                id,
                status: if status == TaskStatus::Done {
                    TaskStatus::Todo
                } else {
                    TaskStatus::Done
                },
                confirmed: false,
            });
        }
    }

    pub(in crate::desktop) fn select_task_status(self: &Rc<Self>, id: TaskId, index: i32) {
        let status = match index {
            0 => TaskStatus::Todo,
            1 => TaskStatus::InProgress,
            2 => TaskStatus::Waiting,
            3 => TaskStatus::Done,
            _ => return,
        };
        self.task_transition(Transition::Status {
            id,
            status,
            confirmed: false,
        });
    }

    pub(in crate::desktop) fn cycle_task_priority(&self, id: TaskId) {
        let metadata = {
            let tasks = self.tasks.borrow();
            tasks
                .editor
                .as_ref()
                .filter(|editor| editor.detail.task.id == id)
                .map(|editor| (editor.detail.task.priority, editor.detail.task.updated_at))
                .or_else(|| {
                    tasks.today.as_ref().and_then(|view| {
                        view.sections
                            .iter()
                            .flat_map(|section| &section.items)
                            .find(|task| task.id == id)
                            .map(|task| (task.priority, task.updated_at))
                    })
                })
        };
        if let Some((priority, expected_updated_at)) = metadata {
            self.queue_task_command(
                Command::UpdateTask {
                    id,
                    patch: TaskPatch {
                        priority: Some(priority.next_priority()),
                        ..Default::default()
                    },
                    expected_updated_at,
                },
                PendingTask::Priority,
            );
        } else {
            self.notice(ToastKind::Error, task_error_message(TaskError::NotFound));
        }
    }

    pub(in crate::desktop) fn defer_task_focus(self: &Rc<Self>) {
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .map(|editor| editor.detail.task.id);
        let Some(id) = selected else {
            return;
        };
        if self.with_ui(|_| ()).is_none() {
            self.emit_ui(UiEffect::Focus(FocusTarget::TaskEditor(id)));
            return;
        }
        let weak = self.page_target();
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                if runtime.tasks.borrow().resume.focus_pending {
                    runtime.defer_task_resume();
                    return;
                }
                runtime.emit_ui(UiEffect::Focus(FocusTarget::TaskEditor(id)));
            }
        });
    }
}
