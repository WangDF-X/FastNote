use std::cell::{Cell, RefCell};

use fastnote_core::{Clock, NaiveDate};
use fastnote_ui::NavigationIssue;

use crate::controller::AppController;
use crate::desktop::ui_session::with_session;

use super::*;

struct FixedClock(NaiveDate);

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.0
    }
}

#[test]
fn test_existing_shell_handles_action_once_without_running_kernel_fallback() {
    let calls = Cell::new(0);
    let result = dispatch_or_handle(
        "navigate:日历",
        |_| {
            calls.set(calls.get() + 1);
            true
        },
        |_| panic!("existing shell already handles the action"),
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(result, None);
}

#[test]
fn test_absent_shell_navigation_updates_full_kernel_route_without_creating_ui() {
    let date = "2026-11-04".parse().unwrap();
    let mut controller = AppController::new(FixedClock(date));
    let ui = RefCell::new(None::<u8>);
    let shell_calls = Cell::new(0);
    let kernel_calls = Cell::new(0);
    let update = dispatch_or_handle(
        "navigate:日历?selected=2026-10-23",
        |_| with_session(&ui, |_| shell_calls.set(shell_calls.get() + 1)).is_some(),
        |action| {
            kernel_calls.set(kernel_calls.get() + 1);
            controller.handle_action(action)
        },
    )
    .unwrap();
    assert_eq!(shell_calls.get(), 0);
    assert_eq!(kernel_calls.get(), 1);
    assert_eq!(*ui.borrow(), None);
    assert!(update.route_changed);
    assert_eq!(controller.shell_view(), update.view);
    assert_eq!(update.view.route_debug, "navigate:日历?selected=2026-10-23");
    assert!(!should_refresh_today(&update, false));
}

#[test]
fn test_absent_shell_non_navigation_action_is_processed_once_not_queued_for_reexecution() {
    let date = "2026-11-04".parse().unwrap();
    let mut controller = AppController::new(FixedClock(date));
    let calls = Cell::new(0);
    let update = dispatch_or_handle(
        "desktop:quit",
        |_| false,
        |action| {
            calls.set(calls.get() + 1);
            controller.handle_action(action)
        },
    )
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert!(!update.route_changed);
    assert!(update.issue.is_none());
    assert!(!should_refresh_today(&update, false));
}

#[test]
fn test_invalid_navigation_without_shell_retains_route_and_returns_safe_issue() {
    let date = "2026-11-04".parse().unwrap();
    let mut controller = AppController::new(FixedClock(date));
    let previous = controller.shell_view();
    let update = dispatch_or_handle(
        "navigate:无效目标",
        |_| false,
        |action| controller.handle_action(action),
    )
    .unwrap();
    assert_eq!(update.view, previous);
    assert!(!update.route_changed);
    assert_eq!(update.issue, Some(NavigationIssue::Target));
    assert!(!should_refresh_today(&update, false));
    assert!(!should_refresh_today(&update, true));
}

#[test]
fn test_today_navigation_only_schedules_queries_for_visible_main_content() {
    let date = "2026-11-04".parse().unwrap();
    let mut controller = AppController::new(FixedClock(date));
    let calendar = controller.handle_action("navigate:日历");
    assert!(!should_refresh_today(&calendar, true));
    let today = controller.handle_action("navigate:今日任务");
    assert!(today.route_changed);
    assert!(!should_refresh_today(&today, false));
    assert!(should_refresh_today(&today, true));
    let unchanged = controller.handle_action("navigate:今日任务");
    assert!(!should_refresh_today(&unchanged, true));
}

#[test]
fn test_entry_dispatch_and_no_session_query_guard_are_wired_to_production_actions() {
    let source = include_str!("../../actions.rs");
    assert!(source.contains("self.dispatch_action(format!(\"navigate:{page}\"));"));
    assert!(source.contains("self.dispatch_action(action);"));
    assert!(source.contains("self.today_dirty.set(true);"));
    assert!(source.contains("if should_refresh_today(&update, self.main_content_visible())"));
    assert!(source.contains("self.defer_page_tasks();"));
    assert!(source.contains("self.set_desktop_state(0);"));
    assert!(source.contains("self.dialog_blocked()"));
}
