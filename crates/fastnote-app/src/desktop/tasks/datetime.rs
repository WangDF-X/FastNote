use std::rc::Rc;

use fastnote_core::{NaiveDate, NaiveDateTime, NaiveTime, TaskError};
use fastnote_ui::{task_error_message, Date, ToastKind};

use crate::desktop::runtime::Runtime;

impl Runtime {
    pub(in crate::desktop) fn select_task_datetime(
        self: &Rc<Self>,
        field: &str,
        date: Date,
        hour: i32,
        minute: i32,
    ) {
        let value = selection(date, hour, minute);
        match value {
            Ok(value) => self.update_task_datetime(field, Some(value)),
            Err(error) => self.notice(ToastKind::Error, task_error_message(error)),
        }
    }

    pub(in crate::desktop) fn update_task_datetime(
        self: &Rc<Self>,
        field: &str,
        value: Option<NaiveDateTime>,
    ) {
        if self.task_syncing.get() || !self.can_task_interact() || !self.can_focus_tasks() {
            return;
        }
        let result = self
            .tasks
            .borrow_mut()
            .editor
            .as_mut()
            .map(|editor| editor.select_datetime(field, value));
        match result {
            Some(Ok(true)) => {
                self.sync_task_editor();
                self.flush_task_editor();
            }
            Some(Err(error)) => self.notice(ToastKind::Error, task_error_message(error)),
            _ => {}
        }
    }
}

fn selection(date: Date, hour: i32, minute: i32) -> Result<NaiveDateTime, TaskError> {
    if !(1900..=9999).contains(&date.year) {
        return Err(TaskError::DateRange);
    }
    let unsigned = |value| u32::try_from(value).map_err(|_| TaskError::DateRange);
    let date = NaiveDate::from_ymd_opt(date.year, unsigned(date.month)?, unsigned(date.day)?)
        .ok_or(TaskError::DateRange)?;
    let time = NaiveTime::from_hms_opt(unsigned(hour)?, unsigned(minute)?, 0)
        .ok_or(TaskError::DateRange)?;
    Ok(date.and_time(time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picker_selection_uses_minute_precision_and_rejects_invalid_components() {
        let date = Date {
            year: 2026,
            month: 11,
            day: 3,
        };
        assert_eq!(
            selection(date.clone(), 15, 31).unwrap().to_string(),
            "2026-11-03 15:31:00"
        );
        for (hour, minute) in [(-1, 0), (24, 0), (12, -1), (12, 60)] {
            assert_eq!(
                selection(date.clone(), hour, minute),
                Err(TaskError::DateRange)
            );
        }
        assert_eq!(
            selection(
                Date {
                    year: 2026,
                    month: 2,
                    day: 29
                },
                9,
                0
            ),
            Err(TaskError::DateRange)
        );
        for year in [1899, 10000] {
            assert_eq!(
                selection(
                    Date {
                        year,
                        month: 1,
                        day: 1
                    },
                    0,
                    0
                ),
                Err(TaskError::DateRange)
            );
        }
        for year in [1900, 9999] {
            assert!(selection(
                Date {
                    year,
                    month: 1,
                    day: 1
                },
                23,
                59
            )
            .is_ok());
        }
    }
}
