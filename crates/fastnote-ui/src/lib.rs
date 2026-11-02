//! Slint-based user interface for FastNote.

slint::include_modules!();

mod desktop;
mod feedback;
mod shell;

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
