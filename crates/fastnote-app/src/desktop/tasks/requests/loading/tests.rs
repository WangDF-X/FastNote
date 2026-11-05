use fastnote_core::{Clock, NaiveDate};

use crate::controller::AppController;
use crate::desktop::tasks::policy::may_query;
use crate::tasks::TaskSession;

use super::*;

struct FixedClock;
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        "2026-11-04".parse().unwrap()
    }
}

#[test]
fn test_hidden_and_non_today_first_entry_enqueue_zero_today_queries() {
    for (content, kernel_page, ui_page) in [
        (false, "今日任务", "今日任务"),
        (true, "日历", "日历"),
        (true, "设置", "设置"),
        (true, "今日任务", "日历"),
    ] {
        let tasks = TaskSession::new();
        let visible = today_page_visible(content, kernel_page, ui_page);
        let count = usize::from(
            may_query(None, visible)
                && needs_page_load(true, tasks.today.is_some(), tasks.latest_today),
        );
        assert_eq!(count, 0);
        assert_eq!(tasks.latest_today, 0);
    }
}

#[test]
fn test_repeated_first_show_and_restore_enqueue_one_in_flight_today_query() {
    let mut tasks = TaskSession::new();
    let mut count = 0;
    for _ in 0..20 {
        if may_query(None, today_page_visible(true, "今日任务", "今日任务"))
            && needs_page_load(true, tasks.today.is_some(), tasks.latest_today)
        {
            count += 1;
            tasks.latest_today = 7;
        }
    }
    assert_eq!(count, 1);
    assert!(!tasks.today_requery);
    assert_eq!(tasks.finish_today_read(7), Some(false));
    assert!(needs_page_load(
        true,
        tasks.today.is_some(),
        tasks.latest_today
    ));
}

#[test]
fn test_first_non_today_route_is_kept_until_today_is_explicitly_requested() {
    let mut controller = AppController::new(FixedClock);
    let calendar = controller.handle_action("navigate:日历?selected=2025-03-17");
    assert!(calendar.route_changed);
    assert!(!today_page_visible(
        true,
        calendar.view.page_label,
        calendar.view.page_label
    ));
    assert_eq!(
        controller.shell_view().route_debug,
        "navigate:日历?selected=2025-03-17"
    );
    let today = controller.handle_action("navigate:今日任务");
    assert!(!today_page_visible(
        true,
        today.view.page_label,
        calendar.view.page_label
    ));
    assert!(today_page_visible(
        true,
        today.view.page_label,
        today.view.page_label
    ));
}

#[test]
fn test_kernel_startup_only_arms_timer_and_loaded_never_starts_page_queries() {
    let source = include_str!("../loading.rs");
    let kernel = source
        .split_once("fn start_kernel_tasks(")
        .unwrap()
        .1
        .split_once("fn start_page_tasks(")
        .unwrap()
        .0;
    let armed = kernel.find(".start(").unwrap();
    assert!(
        !kernel[..armed].contains("request_today("),
        "tasks/requests/loading.rs"
    );
    assert!(
        kernel.contains("Duration::from_secs(30)"),
        "tasks/requests/loading.rs"
    );
    let loaded = include_str!("../../../lifecycle.rs")
        .split_once("fn loaded(")
        .unwrap()
        .1
        .split_once("fn register_hotkey(")
        .unwrap()
        .0;
    assert!(
        loaded.contains("self.start_kernel_tasks();"),
        "lifecycle.rs"
    );
    assert!(!loaded.contains("start_page_tasks("), "lifecycle.rs");
    assert!(!loaded.contains("request_today("), "lifecycle.rs");
}

#[test]
fn test_route_queries_wait_for_projection_and_show_queries_wait_for_real_visibility() {
    let actions = include_str!("../../../actions.rs");
    assert!(actions.contains("self.defer_page_tasks();"), "actions.rs");
    let deferred = include_str!("../loading.rs")
        .split_once("fn defer_page_tasks(")
        .unwrap()
        .1
        .split_once("fn today_page_visible(")
        .unwrap()
        .0;
    assert!(
        deferred.contains("Timer::single_shot(Duration::ZERO"),
        "tasks/requests/loading.rs"
    );
    assert!(
        deferred.contains("self.page_target()"),
        "tasks/requests/loading.rs"
    );
    let presentation = include_str!("../../../presentation/mod.rs");
    let show = presentation
        .split_once("fn try_show_main(")
        .unwrap()
        .1
        .split_once("fn persist_ball_position(")
        .unwrap()
        .0;
    assert!(
        show.find("result?;").unwrap() < show.find("self.start_page_tasks();").unwrap(),
        "presentation/mod.rs"
    );
    let window = include_str!("../../../window.rs");
    let visible = window
        .split_once("fn main_content_visible(")
        .unwrap()
        .1
        .split_once("fn sync_window_state(")
        .unwrap()
        .0;
    assert!(visible.contains("window.is_visible()"), "window.rs");
    assert!(visible.contains("window.is_minimized()"), "window.rs");
}

#[test]
fn test_wakeup_only_loads_on_restore_edge_and_hidden_path_never_queries_or_builds_models() {
    let presentation = include_str!("../../../presentation/mod.rs");
    let resume = presentation
        .split_once("MinimizeAction::Resume => {")
        .unwrap()
        .1
        .split_once("MinimizeAction::RestoreMain")
        .unwrap()
        .0;
    assert!(
        resume.contains("!ui.content_active.replace(true)"),
        "presentation/mod.rs"
    );
    let hidden = include_str!("../loading.rs")
        .split_once("fn task_page_hidden(")
        .unwrap()
        .1
        .split_once("#[cfg(test)]")
        .unwrap()
        .0;
    assert!(
        hidden.contains("tasks.cancel_page_reads();"),
        "tasks/requests/loading.rs"
    );
    assert!(
        hidden.contains("ui.content_active.set(false);"),
        "tasks/requests/loading.rs"
    );
    assert!(
        !hidden.contains("request_today("),
        "tasks/requests/loading.rs"
    );
    assert!(
        !hidden.contains("ensure_page_resources("),
        "tasks/requests/loading.rs"
    );
}
