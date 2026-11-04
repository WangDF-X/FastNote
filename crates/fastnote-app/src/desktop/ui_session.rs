use std::cell::{Cell, RefCell};

use fastnote_ui::{ShellHandle, TodayPresenter};
use slint::Timer;

/// SHELL-03 T1.1：主窗及页面资源的唯一持有者，不强持有 Runtime。
pub(super) struct UiSession {
    pub shell: ShellHandle,
    pub today_presenter: TodayPresenter,
    pub task_save_timer: Timer,
    pub task_feedback_timer: Timer,
    pub task_saving_visible: Cell<bool>,
}

impl UiSession {
    /// 将已装配的主窗与页面模型集中到同一可丢弃会话。
    pub(super) fn new(shell: ShellHandle) -> Self {
        let today_presenter = TodayPresenter::new(shell.component());
        Self {
            shell,
            today_presenter,
            task_save_timer: Timer::default(),
            task_feedback_timer: Timer::default(),
            task_saving_visible: Cell::new(false),
        }
    }
}

impl Drop for UiSession {
    fn drop(&mut self) {
        // 内核的 30 秒日期检测不属于页面；这里只停止页面保存与反馈计时。
        self.task_save_timer.stop();
        self.task_feedback_timer.stop();
    }
}

/// 临时只读访问会话；缺失时不调用投影，也不创建资源。
pub(super) fn with_session<T, R>(
    session: &RefCell<Option<T>>,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    session.borrow().as_ref().map(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_absent_session_does_not_invoke_projection_or_create_resources() {
        let session = RefCell::new(None::<u8>);
        let called = Cell::new(false);
        let result = with_session(&session, |_| called.set(true));
        assert_eq!(result, None);
        assert!(!called.get());
        assert_eq!(*session.borrow(), None);
    }

    #[test]
    fn test_session_access_returns_projection_without_taking_ownership() {
        let session = RefCell::new(Some(7));
        assert_eq!(with_session(&session, |value| value + 1), Some(8));
        assert_eq!(*session.borrow(), Some(7));
        assert_eq!(session.take(), Some(7));
    }

    #[test]
    fn test_session_access_allows_nested_read_only_projection() {
        let session = RefCell::new(Some(7));
        let result = with_session(&session, |outer| {
            with_session(&session, |inner| outer + inner)
        });
        assert_eq!(result, Some(Some(14)));
        assert_eq!(session.take(), Some(7));
    }

    #[test]
    fn test_ui_resources_are_owned_by_session_and_date_timer_stays_in_kernel() {
        // 仅检查资源归属，不构造真实 MainWindow。
        let runtime = include_str!("runtime.rs")
            .split_once("pub(super) struct Runtime {")
            .unwrap()
            .1
            .split_once("\n}")
            .unwrap()
            .0;
        assert!(
            runtime.contains("pub ui: RefCell<Option<UiSession>>"),
            "runtime.rs"
        );
        assert!(runtime.contains("pub task_day_timer: Timer"), "runtime.rs");
        for field in [
            "shell",
            "today_presenter",
            "task_save_timer",
            "task_feedback_timer",
            "task_saving_visible",
        ] {
            assert!(
                !runtime.contains(&format!("pub {field}:")),
                "runtime.rs: {field}"
            );
            let session = include_str!("ui_session.rs")
                .split_once("impl UiSession {")
                .unwrap()
                .0;
            assert!(
                session.contains(&format!("pub {field}:")),
                "ui_session.rs: {field}"
            );
        }
    }

    #[test]
    fn test_ui_session_drop_stops_both_owned_page_timers() {
        let source = include_str!("ui_session.rs")
            .split_once("impl Drop for UiSession {")
            .unwrap()
            .1
            .split_once("/// 临时只读访问会话")
            .unwrap()
            .0;
        assert!(
            source.contains("self.task_save_timer.stop();"),
            "ui_session.rs"
        );
        assert!(
            source.contains("self.task_feedback_timer.stop();"),
            "ui_session.rs"
        );
        assert!(!source.contains("task_day_timer"), "ui_session.rs");
        assert!(
            include_str!("lifecycle.rs").contains("self.task_day_timer.stop();"),
            "lifecycle.rs"
        );
    }
}
