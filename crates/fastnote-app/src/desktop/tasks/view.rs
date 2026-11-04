use fastnote_core::{TaskClock, TaskError, TaskId};
use fastnote_store::{StoreError, TaskStoreError};
use fastnote_ui::{
    storage_message, task_context_label, task_datetime_seed, task_error_message, MainWindow,
    ShellFocus,
};
use slint::ComponentHandle;

use crate::clock::SystemClock;
use crate::desktop::runtime::Runtime;
use crate::tasks::DraftFields;

pub(super) fn error_message(error: &TaskStoreError) -> String {
    if let Some(error) = error.domain_error() {
        return task_error_message(error).to_owned();
    }
    match error.storage_error() {
        Some(error) => {
            let message = if matches!(error, StoreError::NewerSchema { .. }) {
                "数据由更高版本创建，请升级应用。"
            } else {
                storage_message(error.code())
            };
            format!("{message}\n日志 ID：{}", error.log_id())
        }
        None => task_error_message(TaskError::Conflict).to_owned(),
    }
}

impl Runtime {
    pub(in crate::desktop) fn sync_task_editor(&self) {
        // Slint 属性变更可能同步触发失焦回调；不能跨这些调用持有 RefCell 借用。
        let saving_visible = self
            .with_ui(|ui| ui.task_saving_visible.get())
            .unwrap_or(false);
        let snapshot = {
            let tasks = self.tasks.borrow();
            let busy = !tasks.pending.is_empty()
                || tasks.non_text_pending()
                || tasks
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.in_flight.is_some());
            let enabled = self.ready.get()
                && !self.closing.get()
                && !self.busy.get()
                && !tasks.non_text_pending();
            let editor = tasks.editor.as_ref().map(|editor| {
                (
                    editor.detail.task.id,
                    editor.fields.clone(),
                    editor.detail.context.read_only,
                    task_context_label(&editor.detail),
                    editor.error.clone().unwrap_or_default(),
                    if editor.error.is_some() {
                        if editor.committed {
                            "刷新失败"
                        } else {
                            "保存失败"
                        }
                    } else if editor.detail.context.read_only {
                        "只读"
                    } else if editor.in_flight.is_some() {
                        if saving_visible {
                            "保存中…"
                        } else {
                            "自动保存"
                        }
                    } else if editor.dirty() {
                        "待保存"
                    } else {
                        "已保存"
                    },
                )
            });
            (busy, enabled, editor)
        };
        self.task_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_task_busy(snapshot.0);
            window.set_task_editor_enabled(snapshot.1);
            if let Some((id, fields, read_only, context, error, label)) = snapshot.2 {
                window.set_task_selected_id(id.to_string().into());
                apply_fields(window, fields);
                window.set_task_read_only(read_only);
                window.set_task_context(context.into());
                window.set_task_error(error.into());
                window.set_task_save_label(label.into());
            }
        });
        self.task_syncing.set(false);
    }

    pub(in crate::desktop) fn apply_today(&self) {
        let (view, collapsed) = {
            let tasks = self.tasks.borrow();
            (tasks.today.clone(), tasks.collapsed)
        };
        if let Some(view) = view {
            let _ = self.with_ui(|ui| {
                ui.today_presenter
                    .apply(ui.shell.component(), &view, collapsed);
            });
        }
        self.restore_task_list_focus();
    }

    pub(in crate::desktop) fn restore_task_list_focus(&self) {
        let _ = self.with_ui(|ui| {
            let anchor = ui.shell.component().global::<ShellFocus>().get_anchor();
            if anchor.starts_with("task:") {
                let id = anchor
                    .rsplit(':')
                    .next()
                    .and_then(|value| value.parse::<TaskId>().ok());
                let visible = id.is_some_and(|id| self.tasks.borrow().visible_ids().contains(&id));
                if visible {
                    ui.shell.component().invoke_restore_task_focus();
                } else {
                    ui.shell.component().invoke_focus_task_create();
                }
            } else if anchor.starts_with("today:section:") || anchor.starts_with("today:more:") {
                let index = anchor
                    .rsplit(':')
                    .next()
                    .and_then(|value| value.parse::<usize>().ok());
                let exists = index.is_some_and(|index| {
                    self.tasks.borrow().today.as_ref().is_some_and(|view| {
                        view.sections
                            .iter()
                            .any(|section| section.kind as usize == index)
                    })
                });
                if exists {
                    ui.shell
                        .component()
                        .global::<ShellFocus>()
                        .set_anchor(format!("today:section:{}", index.unwrap_or(0)).into());
                    ui.shell.component().invoke_restore_task_focus();
                } else {
                    ui.shell.component().invoke_focus_task_create();
                }
            }
        });
    }

    pub(in crate::desktop) fn close_task_editor(&self) {
        let _ = self.with_ui(|ui| {
            ui.task_save_timer.stop();
            ui.task_feedback_timer.stop();
        });
        let selected = {
            let mut tasks = self.tasks.borrow_mut();
            tasks.cancel_open_requests();
            tasks.editor.take().map(|editor| editor.detail.task.id)
        };
        self.task_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_task_detail_visible(false);
            window.set_task_detail_loading(false);
            window.set_task_selected_id("".into());
            window.set_task_error("".into());
        });
        self.task_syncing.set(false);
        self.sync_task_editor();
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if selected.is_some_and(|id| self.tasks.borrow().visible_ids().contains(&id)) {
                if let Some(id) = selected {
                    window.invoke_focus_task_row(id.to_string().into());
                }
            } else {
                window.invoke_focus_task_create();
            }
        });
    }
}

fn apply_fields(window: &MainWindow, fields: DraftFields) {
    let now = SystemClock.local_datetime(SystemClock.now());
    window.set_task_picker_today(task_datetime_seed("", "", now).date);
    let start = task_datetime_seed(&fields.start_date, &fields.start_time, now);
    let due = task_datetime_seed(&fields.due_date, &fields.due_time, now);
    window.set_task_start_seed_date(start.date);
    window.set_task_start_seed_hour(start.hour);
    window.set_task_start_seed_minute(start.minute);
    window.set_task_due_seed_date(due.date);
    window.set_task_due_seed_hour(due.hour);
    window.set_task_due_seed_minute(due.minute);
    window.set_task_title(fields.title.into());
    window.set_task_description_count(
        i32::try_from(fields.description.chars().count()).unwrap_or(i32::MAX),
    );
    window.set_task_description(fields.description.into());
    window.set_task_start_date(fields.start_date.into());
    window.set_task_start_time(fields.start_time.into());
    window.set_task_due_date(fields.due_date.into());
    window.set_task_due_time(fields.due_time.into());
}
