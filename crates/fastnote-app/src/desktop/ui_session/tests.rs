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
    let runtime = include_str!("../runtime.rs")
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
    let session = include_str!("../ui_session.rs")
        .split_once("impl UiSession {")
        .unwrap()
        .0;
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
        assert!(
            session.contains(&format!("pub {field}:")),
            "ui_session.rs: {field}"
        );
    }
}

#[test]
fn test_ui_session_drop_stops_both_owned_page_timers() {
    let source = include_str!("../ui_session.rs")
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
        include_str!("../lifecycle.rs").contains("self.task_day_timer.stop();"),
        "lifecycle.rs"
    );
}

#[test]
fn test_route_projection_matches_shell_view_without_reexecuting_actions() {
    let app = include_str!("../ui_session.rs")
        .split_once("impl Drop for UiSession")
        .unwrap()
        .0;
    let shared = include_str!("../../../../fastnote-ui/src/shell/view.rs");
    for setter in [
        "set_current_page",
        "set_page_title",
        "set_route_context",
        "set_route_debug",
        "set_can_go_back",
        "set_plan_detail",
        "set_general_settings",
        "set_settings_section",
    ] {
        assert!(app.contains(setter), "ui_session.rs: {setter}");
        assert!(shared.contains(setter), "shell/view.rs: {setter}");
    }
    assert!(!app.contains(".dispatch("), "ui_session.rs");
    assert!(!app.contains("handle_action("), "ui_session.rs");
}

#[test]
fn test_non_today_routes_never_construct_today_models() {
    let slot = RefCell::new(None::<u8>);
    for page in ["日历", "设置", "小记", "日记", "计划板", "无效页面"] {
        assert!(!ensure_page_resource(&slot, page, || panic!(
            "non-Today page must not preload Today"
        )));
    }
    assert_eq!(*slot.borrow(), None);
}

#[test]
fn test_first_today_entry_constructs_once_and_other_routes_keep_the_cached_resource() {
    let slot = RefCell::new(None::<u8>);
    let calls = Cell::new(0);
    assert!(ensure_page_resource(&slot, "今日任务", || {
        calls.set(calls.get() + 1);
        7
    }));
    for page in ["今日任务", "日历", "今日任务"] {
        assert!(!ensure_page_resource(&slot, page, || {
            calls.set(calls.get() + 1);
            9
        }));
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(*slot.borrow(), Some(7));
}

#[test]
fn test_page_factory_does_not_hold_optional_resource_borrow() {
    let slot = RefCell::new(None::<u8>);
    assert!(ensure_page_resource(&slot, "今日任务", || {
        assert!(slot.borrow_mut().is_none());
        7
    }));
    assert_eq!(*slot.borrow(), Some(7));
}

#[test]
fn test_shell_session_constructor_and_reply_projection_do_not_create_page_models() {
    let source = include_str!("../ui_session.rs");
    let constructor = source
        .split_once("fn new(")
        .unwrap()
        .1
        .split_once("fn ensure_page_resources(")
        .unwrap()
        .0;
    assert!(
        constructor.contains("today_presenter: RefCell::new(None)"),
        "ui_session.rs"
    );
    assert!(
        !constructor.contains("TodayPresenter::new("),
        "ui_session.rs"
    );
    let projection = source
        .split_once("fn apply_today(")
        .unwrap()
        .1
        .split_once("fn apply_route_view(")
        .unwrap()
        .0;
    assert!(
        !projection.contains("TodayPresenter::new("),
        "ui_session.rs"
    );
    assert!(
        !projection.contains("ensure_page_resources("),
        "ui_session.rs"
    );
}
