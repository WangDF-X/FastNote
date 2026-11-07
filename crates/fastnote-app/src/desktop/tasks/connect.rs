use std::rc::Rc;

use crate::desktop::runtime::Runtime;
use crate::tasks::Transition;

impl Runtime {
    pub(in crate::desktop) fn connect_tasks(self: &Rc<Self>) {
        self.connect_calendar();
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.on_task_create({
                let weak = self.ui_target();
                move |title| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.create_task(title.to_string());
                    }
                }
            });
            window.on_task_open({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.task_transition(Transition::Open(id));
                    }
                }
            });
            window.on_task_toggle({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.toggle_task(id);
                    }
                }
            });
            window.on_task_open_dates({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.task_transition(Transition::OpenDates(id));
                    }
                }
            });
            window.on_task_delete({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.task_transition(Transition::Delete(id));
                    }
                }
            });
            window.on_task_priority({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.task_transition(Transition::Priority(id));
                    }
                }
            });
            window.on_task_focus_move({
                let weak = self.ui_target();
                move |id, direction| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        if !runtime.can_task_interact() || !runtime.can_focus_tasks() {
                            return;
                        }
                        let neighbor = runtime.tasks.borrow().focus_neighbor(id, direction);
                        if let Some(id) = neighbor {
                            let _ = runtime.with_ui(|ui| {
                                ui.shell
                                    .component()
                                    .invoke_focus_task_row(id.to_string().into());
                            });
                        }
                    }
                }
            });
            window.on_task_edit({
                let weak = self.ui_target();
                move |field, value| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.edit_task_field(field.as_str(), value.to_string());
                    }
                }
            });
            window.on_task_set_status({
                let weak = self.ui_target();
                move |id, index| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.select_task_status(id, index);
                    }
                }
            });
            window.on_task_datetime({
                let weak = self.ui_target();
                move |field, date, hour, minute| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.select_task_datetime(field.as_str(), date, hour, minute);
                    }
                }
            });
            window.on_task_clear_datetime({
                let weak = self.ui_target();
                move |field| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.update_task_datetime(field.as_str(), None);
                    }
                }
            });
            window.on_task_flush({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        if !runtime.task_syncing.get() {
                            runtime.flush_task_editor();
                        }
                    }
                }
            });
            window.on_task_close({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.task_transition(Transition::CloseDetail);
                    }
                }
            });
            window.on_task_retry({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.retry_task_save();
                    }
                }
            });
            window.on_task_reload({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.request_editor_reload();
                    }
                }
            });
            window.on_today_retry({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.request_today();
                    }
                }
            });
            window.on_today_section({
                let weak = self.ui_target();
                move |index| {
                    if let (Some(runtime), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
                        if !runtime.can_task_interact() || !runtime.can_focus_tasks() {
                            return;
                        }
                        let changed = runtime.tasks.borrow_mut().toggle_section(index);
                        if changed {
                            runtime.apply_today();
                        }
                    }
                }
            });
            window.on_today_more({
                let weak = self.ui_target();
                move |index| {
                    if let (Some(runtime), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
                        if !runtime.can_task_interact() || !runtime.can_focus_tasks() {
                            return;
                        }
                        let changed = runtime.tasks.borrow_mut().load_more(index);
                        if changed {
                            runtime.request_today();
                        }
                    }
                }
            });
        });
    }
}
