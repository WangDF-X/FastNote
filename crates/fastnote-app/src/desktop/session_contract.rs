const SOURCES: &[(&str, &str)] = &[
    ("actions.rs", include_str!("actions.rs")),
    ("actions/dispatch.rs", include_str!("actions/dispatch.rs")),
    ("ball.rs", include_str!("ball.rs")),
    ("close.rs", include_str!("close.rs")),
    ("effects.rs", include_str!("effects.rs")),
    ("gesture.rs", include_str!("gesture.rs")),
    ("lifecycle.rs", include_str!("lifecycle.rs")),
    (
        "lifecycle/recovery.rs",
        include_str!("lifecycle/recovery.rs"),
    ),
    ("mod.rs", include_str!("mod.rs")),
    ("presentation/mod.rs", include_str!("presentation/mod.rs")),
    (
        "presentation/state.rs",
        include_str!("presentation/state.rs"),
    ),
    ("runtime.rs", include_str!("runtime.rs")),
    ("runtime/session.rs", include_str!("runtime/session.rs")),
    ("task_preferences.rs", include_str!("task_preferences.rs")),
    ("tasks/autosave.rs", include_str!("tasks/autosave.rs")),
    ("tasks/connect.rs", include_str!("tasks/connect.rs")),
    ("tasks/datetime.rs", include_str!("tasks/datetime.rs")),
    ("tasks/mod.rs", include_str!("tasks/mod.rs")),
    ("tasks/policy.rs", include_str!("tasks/policy.rs")),
    ("tasks/replies.rs", include_str!("tasks/replies.rs")),
    (
        "tasks/replies/failures.rs",
        include_str!("tasks/replies/failures.rs"),
    ),
    (
        "tasks/replies/state.rs",
        include_str!("tasks/replies/state.rs"),
    ),
    ("tasks/requests.rs", include_str!("tasks/requests.rs")),
    (
        "tasks/requests/timing.rs",
        include_str!("tasks/requests/timing.rs"),
    ),
    (
        "tasks/requests/loading.rs",
        include_str!("tasks/requests/loading.rs"),
    ),
    (
        "tasks/replies/today.rs",
        include_str!("tasks/replies/today.rs"),
    ),
    ("tasks/transitions.rs", include_str!("tasks/transitions.rs")),
    (
        "tasks/transitions/confirmations.rs",
        include_str!("tasks/transitions/confirmations.rs"),
    ),
    ("tasks/view.rs", include_str!("tasks/view.rs")),
    ("tasks/view/state.rs", include_str!("tasks/view/state.rs")),
    ("theme.rs", include_str!("theme.rs")),
    ("ui_session.rs", include_str!("ui_session.rs")),
    ("wakeup.rs", include_str!("wakeup.rs")),
    ("window.rs", include_str!("window.rs")),
    ("window/ui_events.rs", include_str!("window/ui_events.rs")),
    ("window_frame.rs", include_str!("window_frame.rs")),
    ("worker.rs", include_str!("worker.rs")),
];

#[test]
fn test_desktop_sources_have_no_retired_direct_main_shell_access() {
    for (file, source) in SOURCES {
        for prefix in ["self", "runtime"] {
            let retired = [prefix, ".", "shell", "."].concat();
            assert!(!source.contains(&retired), "{file}: {retired}");
        }
    }
}

#[test]
fn test_normal_hidden_runtime_starts_absent_and_only_initializes_ui_when_requested() {
    let runtime = include_str!("runtime.rs");
    let compact = runtime.split_whitespace().collect::<String>();
    assert!(compact.contains("ui:RefCell::new(None)"), "runtime.rs");
    assert!(
        compact.contains("if!hidden{runtime.ensure_ui()?;}"),
        "runtime.rs"
    );
    assert!(!compact.contains("ShellHandle::new("), "runtime.rs");
    assert!(!compact.contains("TodayPresenter::new("), "runtime.rs");
    let present = include_str!("presentation/mod.rs");
    let show = present
        .split_once("fn show_main(")
        .unwrap()
        .1
        .split_once("fn persist_ball_position(")
        .unwrap()
        .0;
    assert!(
        show.find("self.ensure_ui()").unwrap() < show.find("self.switch_presentation(").unwrap(),
        "presentation/mod.rs"
    );
}

#[test]
fn test_ui_resources_and_timer_cleanup_have_one_owner_with_kernel_date_timer_separate() {
    let runtime = include_str!("runtime.rs")
        .split_once("struct Runtime {")
        .unwrap()
        .1
        .split_once("impl Runtime {")
        .unwrap()
        .0;
    let session = include_str!("ui_session.rs");
    let fields = session.split_once("impl UiSession {").unwrap().0;
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
            fields.contains(&format!("pub {field}:")),
            "ui_session.rs: {field}"
        );
    }
    assert!(
        runtime.contains("pub ui: RefCell<Option<UiSession>>"),
        "runtime.rs"
    );
    assert!(runtime.contains("pub task_day_timer: Timer"), "runtime.rs");
    let dropped = session
        .split_once("impl Drop for UiSession {")
        .unwrap()
        .1
        .split_once("/// 临时只读访问会话")
        .unwrap()
        .0;
    assert!(
        dropped.contains("self.task_save_timer.stop();"),
        "ui_session.rs"
    );
    assert!(
        dropped.contains("self.task_feedback_timer.stop();"),
        "ui_session.rs"
    );
    assert!(!dropped.contains("self.task_day_timer"), "ui_session.rs");
    assert!(
        include_str!("lifecycle.rs").contains("self.task_day_timer.stop();"),
        "lifecycle.rs"
    );
}

#[test]
fn test_main_construction_is_localized_and_never_hidden_in_reply_or_effect_paths() {
    let mut factories = Vec::new();
    for (file, source) in SOURCES {
        for _ in source.match_indices("ShellHandle::new(") {
            factories.push(*file);
        }
        assert!(!source.contains("MainWindow::new("), "{file}");
    }
    assert_eq!(factories.len(), 1, "main factories: {factories:?}");
    assert!(factories[0].starts_with("runtime"), "{factories:?}");
    for file in [
        "effects.rs",
        "tasks/replies.rs",
        "tasks/replies/failures.rs",
    ] {
        let source = SOURCES.iter().find(|(name, _)| *name == file).unwrap().1;
        assert!(!source.contains("ensure_ui("), "{file}");
    }
}

#[test]
fn test_event_loop_and_callback_bindings_do_not_strongly_own_kernel_or_main_window() {
    let wakeup = include_str!("wakeup.rs");
    assert!(wakeup.contains("RefCell<Weak<Runtime>>"), "wakeup.rs");
    assert!(
        wakeup.contains("slint::invoke_from_event_loop(dispatch)"),
        "wakeup.rs"
    );
    assert!(!wakeup.contains(".as_weak()"), "wakeup.rs");
    let events = include_str!("window/ui_events.rs");
    assert!(
        events.contains("ui.callbacks_connected.replace(true)"),
        "window/ui_events.rs"
    );
    assert!(
        events.contains("Rc::downgrade(self)"),
        "window/ui_events.rs"
    );
    assert!(!events.contains("self.clone()"), "window/ui_events.rs");
    assert!(!events.contains("window.as_weak()"), "window/ui_events.rs");
}
