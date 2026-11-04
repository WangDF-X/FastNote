use fastnote_core::{QueryResult, TaskError, TodaySectionKind};
use fastnote_store::TaskStoreError;

use crate::desktop::effects::UiEffect;
use crate::desktop::runtime::Runtime;

use super::super::view::error_message;
use super::state::complete_today;

impl Runtime {
    pub(super) fn today_queried(&self, id: u64, result: Result<QueryResult, TaskStoreError>) {
        let page_visible = self.today_page_visible();
        let (view, error) = match result {
            Ok(QueryResult::Today(view)) => (Some(view), None),
            Err(error) => (None, Some(error)),
            _ => (None, Some(TaskError::Conflict.into())),
        };
        let completion = complete_today(&mut self.tasks.borrow_mut(), id, view, page_visible);
        let Some(completion) = completion else {
            return;
        };
        self.today_dirty.set(!completion.applied);
        if !page_visible {
            return;
        }
        if completion.requery {
            self.request_today();
            return;
        }
        if let Some(error) = error {
            let _ = self.with_ui(|ui| {
                ui.shell.component().set_today_state(2);
                ui.shell
                    .component()
                    .set_today_error(error_message(&error).into());
                ui.shell.component().set_today_error_log_id(
                    error
                        .storage_error()
                        .map_or("", |error| error.log_id())
                        .into(),
                );
            });
            return;
        }
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_today_state(1);
            ui.shell.component().set_today_error("".into());
            ui.shell.component().set_today_error_log_id("".into());
        });
        self.emit_ui(UiEffect::RefreshToday);
        if self.tasks.borrow_mut().take_unscheduled_reveal() {
            let _ = self.with_ui(|ui| {
                ui.shell
                    .component()
                    .invoke_reveal_task_section(TodaySectionKind::Unscheduled as i32);
            });
        }
    }
}
