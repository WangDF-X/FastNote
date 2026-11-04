pub(super) mod state;

use fastnote_core::{TaskClock, TaskError, TaskId};
use fastnote_store::{StoreError, TaskStoreError};
use fastnote_ui::{
    storage_message, task_context_label, task_datetime_seed, task_error_message, MainWindow,
    ShellFocus,
};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::clock::SystemClock;
use crate::desktop::effects::{FocusTarget, UiEffect};
use crate::desktop::runtime::Runtime;
use crate::tasks::DraftFields;

use super::policy::may_focus_tasks;
use state::{close_editor, discard_for_reload, editor_focus_matches, while_syncing};

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
        if !self.today_page_visible() {
            self.detail_dirty.set(true);
            return;
        }
        // Slint 属性变更可能同步触发失焦回调；不能跨这些调用持有 RefCell 借用。
        let projected = self.with_ui(|ui| {
            let saving_visible = ui.task_saving_visible.get();
            let snapshot = {
                let tasks = self.tasks.borrow();
                let busy = !tasks.pending.is_empty()
                    || tasks.non_text_pending()
                    || tasks
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.in_flight.is_some());
                let enabled = self.ready.get()
                    && !self.finished.get()
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
                (busy, enabled, editor, tasks.latest_open != 0)
            };
            while_syncing(&self.task_syncing, || {
                let window = ui.shell.component();
                window.set_task_busy(snapshot.0);
                window.set_task_editor_enabled(snapshot.1);
                window.set_task_detail_loading(snapshot.3);
                window.set_task_detail_visible(snapshot.2.is_some() || snapshot.3);
                if let Some((id, fields, read_only, context, error, label)) = snapshot.2 {
                    window.set_task_selected_id(id.to_string().into());
                    apply_fields(window, fields);
                    window.set_task_read_only(read_only);
                    window.set_task_context(context.into());
                    window.set_task_error(error.into());
                    window.set_task_save_label(label.into());
                }
            });
        });
        if projected.is_none() {
            self.detail_dirty.set(true);
        }
    }

    pub(in crate::desktop) fn apply_today(&self) {
        if !self.today_page_visible() {
            self.today_dirty.set(true);
            return;
        }
        let projected = self.with_ui(|ui| {
            let (view, collapsed) = {
                let tasks = self.tasks.borrow();
                (tasks.today.clone(), tasks.collapsed)
            };
            view.is_some_and(|view| ui.apply_today(&view, collapsed))
        });
        if projected != Some(true) {
            self.today_dirty.set(true);
            return;
        }
        self.restore_task_list_focus();
    }

    pub(in crate::desktop) fn restore_task_list_focus(&self) {
        if !self.can_focus_tasks() {
            return;
        }
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
        let selected = close_editor(&mut self.tasks.borrow_mut());
        self.detail_dirty.set(true);
        let _ = self.with_ui(|ui| {
            ui.task_save_timer.stop();
            ui.task_feedback_timer.stop();
            while_syncing(&self.task_syncing, || {
                let window = ui.shell.component();
                window.set_task_detail_visible(false);
                window.set_task_detail_loading(false);
                window.set_task_selected_id("".into());
                window.set_task_error("".into());
            });
        });
        self.sync_task_editor();
        let target = selected
            .map(FocusTarget::TaskRow)
            .unwrap_or(FocusTarget::TaskCreate);
        self.emit_ui(UiEffect::Focus(target));
    }

    /// 读取已入队后执行用户明确允许的草稿丢弃；保留最新打开请求和加载态。
    pub(super) fn discard_task_editor_for_reload(&self, id: TaskId) -> bool {
        if !discard_for_reload(&mut self.tasks.borrow_mut(), id) {
            self.tasks.borrow_mut().cancel_open_requests();
            return false;
        }
        let _ = self.with_ui(|ui| {
            ui.task_save_timer.stop();
            ui.task_feedback_timer.stop();
        });
        self.sync_task_editor();
        true
    }

    /// 页面焦点不根据会话存在就恢复：隐藏、最小化、路由变化和模态都必须阻断。
    pub(in crate::desktop) fn can_focus_tasks(&self) -> bool {
        if !self.main_visible() || self.finished.get() || self.closing.get() {
            return false;
        }
        if self.controller.borrow().shell_view().page_label != "今日任务" {
            return false;
        }
        self.with_ui(|ui| {
            let window = ui.shell.component();
            let minimized = window
                .window()
                .with_winit_window(|window| window.is_minimized());
            may_focus_tasks(
                true,
                minimized.is_some(),
                minimized.flatten(),
                window.get_current_page().as_str(),
                self.dialog_blocked(),
            )
        })
        .unwrap_or(false)
    }

    /// 只投影匹配的编辑器焦点，不发起读取，也不构造主窗。
    pub(in crate::desktop) fn focus_task_editor(&self, id: TaskId) {
        if !self.can_focus_tasks() {
            return;
        }
        let selected = self
            .tasks
            .borrow()
            .editor
            .as_ref()
            .map(|editor| editor.detail.task.id);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if editor_focus_matches(
                id,
                selected,
                window.get_task_detail_visible(),
                window.get_task_detail_loading(),
            ) {
                window.invoke_focus_task_editor();
            }
        });
    }

    /// 关闭详情后的焦点只回到仍可见的行；新编辑器或其它页面不能被旧意图抢焦点。
    pub(in crate::desktop) fn focus_task_row(&self, id: TaskId) {
        if !self.can_focus_tasks() || self.tasks.borrow().editor.is_some() {
            return;
        }
        let visible = self.tasks.borrow().visible_ids().contains(&id);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if window.get_task_detail_visible() {
                return;
            }
            if visible {
                window.invoke_focus_task_row(id.to_string().into());
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
