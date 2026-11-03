//! 全产品唯一的任务派生规则。

mod today;

pub use today::{build_today_view, derive_task_summary, is_overdue, today_section};
