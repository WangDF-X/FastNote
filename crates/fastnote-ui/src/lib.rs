//! Slint-based user interface for FastNote.

slint::include_modules!();

mod desktop;
mod feedback;
mod shell;
mod theme;
mod today;

pub use desktop::storage_message;
pub use feedback::banner::{BannerAction, BannerKind, BannerRequest, BannerSet};
pub use feedback::dialog::{
    DialogFocus, DialogKind, DialogOutcome, DialogQueue, DialogRequest, OpenOutcome, Resolution,
    CANCEL_LABEL,
};
pub use feedback::toast::{
    PushOutcome, ToastAction, ToastItem, ToastKind, ToastQueue, ToastRequest, DEDUP_WINDOW_MS,
    MAX_VISIBLE_TOASTS,
};
pub use shell::{run, NavigationIssue, ShellHandle, ShellUpdate, ShellView};
pub use theme::apply_theme;
pub use today::{
    task_context_label, task_datetime_seed, task_error_message, task_status_index,
    TaskDateTimeSeed, TodayPresenter,
};
