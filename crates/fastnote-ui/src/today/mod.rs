//! 今日 DTO 的显示投影；不重复判定分组、逾期、等待或排序。

mod calendar;
mod datetime;
mod presentation;
mod summary;

pub use datetime::{task_datetime_seed, TaskDateTimeSeed};
pub(crate) use presentation::row_data;
pub use presentation::{task_context_label, task_error_message, task_status_index, TodayPresenter};

#[cfg(test)]
mod layout_contract;
#[cfg(test)]
mod style_contract;
