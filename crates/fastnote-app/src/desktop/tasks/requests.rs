use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{
    Clock, Command, Query, Task, TaskClock, TaskError, TaskId, TaskPatch, TaskStatus,
};
use fastnote_ui::{task_error_message, ToastKind};
use slint::{Timer, TimerMode};
use uuid::Uuid;

use crate::clock::SystemClock;
use crate::desktop::runtime::Runtime;
use crate::tasks::{DetailPurpose, PendingTask, Transition};

impl Runtime {
    pub(in crate::desktop) fn start_tasks(self: &Rc<Self>) {
        self.request_today();
        let weak = Rc::downgrade(self);
        self.task_day_timer
            .start(TimerMode::Repeated, Duration::from_secs(30), move || {
                if let Some(runtime) = weak.upgrade() {
                    let changed = runtime
                        .tasks
                        .borrow()
                        .needs_clock_refresh(SystemClock.local_datetime(SystemClock.now()));
                    if changed {
                        runtime.request_today();
                        let selected = runtime
                            .tasks
                            .borrow()
                            .editor
                            .as_ref()
                            .map(|editor| editor.detail.task.id);
                        if let Some(id) = selected {
                            runtime.request_task_detail(DetailPurpose::Refresh(id));
                        }
                    }
                }
            });
    }

    pub(in crate::desktop) fn can_task_interact(&self) -> bool {
        self.ready.get()
            && !self.finished.get()
            && !self.closing.get()
            && !self.busy.get()
            && !self.shell.component().get_dialog_visible()
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
        if !self.ready.get() || self.finished.get() {
            return;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        let limits = self.tasks.borrow().limits;
        if self
            .worker
            .query(
                id,
                Query::Today {
                    date: SystemClock.today(),
                    limits,
                },
            )
            .is_err()
        {
            self.shell.component().set_today_state(2);
            self.shell
                .component()
                .set_today_error("查询队列不可用，请稍后重试。".into());
            self.shell.component().set_today_error_log_id("".into());
            return;
        }
        self.tasks.borrow_mut().latest_today = id;
        if self.tasks.borrow().today.is_none() {
            self.shell.component().set_today_state(0);
        }
    }

    pub(in crate::desktop) fn request_task_detail(&self, purpose: DetailPurpose) -> bool {
        if !self.ready.get() || self.finished.get() {
            return false;
        }
        let task_id = match purpose {
            DetailPurpose::Open(id)
            | DetailPurpose::Refresh(id)
            | DetailPurpose::SaveAck(id)
            | DetailPurpose::Delete(id) => id,
        };
        if matches!(
            purpose,
            DetailPurpose::SaveAck(_) | DetailPurpose::Refresh(_)
        ) && self.tasks.borrow().queries.values().any(|pending| {
            matches!((purpose, pending),
                (DetailPurpose::SaveAck(a), DetailPurpose::SaveAck(b))
                    | (DetailPurpose::Refresh(a), DetailPurpose::Refresh(b)) if a == *b)
        }) {
            return true;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self.worker.query(id, Query::TaskDetail(task_id)).is_err() {
            let message = "任务查询队列不可用，请稍后重试。";
            if matches!(purpose, DetailPurpose::SaveAck(_)) {
                if let Some(editor) = self.tasks.borrow_mut().editor.as_mut() {
                    editor.error = Some(message.to_owned());
                }
            }
            self.notice(ToastKind::Error, message);
            self.sync_task_editor();
            return false;
        }
        {
            let mut tasks = self.tasks.borrow_mut();
            tasks.queries.insert(id, purpose);
            match purpose {
                DetailPurpose::Open(_) => tasks.latest_open = id,
                DetailPurpose::Delete(_) => tasks.latest_delete = id,
                _ => {}
            }
        }
        if matches!(purpose, DetailPurpose::Open(_)) {
            self.task_syncing.set(true);
            self.shell.component().set_task_detail_visible(true);
            self.shell.component().set_task_detail_loading(true);
            self.task_syncing.set(false);
        }
        self.sync_task_editor();
        true
    }

    pub(in crate::desktop) fn create_task(self: &Rc<Self>, title: String) {
        if !self.can_task_interact() {
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
            self.shell
                .component()
                .set_task_create_error(task_error_message(error).into());
            return;
        }
        self.shell.component().set_task_create_error("".into());
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
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                runtime.shell.component().invoke_focus_task_editor();
            }
        });
    }
}
