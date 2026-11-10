//! 由统一服务产出的只读视图；UI 不自行推导业务状态。

mod note;
mod task;
mod today;

pub use note::{
    NoteDateGroup, NoteDetail, NoteFileEntry, NoteListPage, NoteListQuery, NoteSummary,
};
pub use task::{TaskContext, TaskDetail, TaskSummary};
pub use today::{TodayPreferences, TodaySection, TodaySectionKind, TodaySort, TodayView};
