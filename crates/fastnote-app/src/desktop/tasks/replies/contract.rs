#[test]
fn test_command_state_and_invalidations_are_updated_before_success_projection() {
    let replies = include_str!("../replies.rs");
    let success = replies
        .split_once("self.mark_task_invalidations(&result.events);")
        .unwrap()
        .1;
    assert!(
        success.find("if preferences.show_unscheduled {").unwrap()
            < success.find("get_task_quick_title()").unwrap(),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("let selected = mark_save_committed("),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("DetailPurpose::SaveAck(id)"),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("acknowledge_save(&mut self.tasks.borrow_mut(), id, detail)"),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("self.emit_ui(UiEffect::Toast(request))"),
        "tasks/replies.rs"
    );
}

#[test]
fn test_page_reply_gate_precedes_model_creation_and_confirmation_is_kept_in_kernel() {
    let replies = include_str!("../replies.rs");
    assert!(
        replies
            .find("if !may_apply_detail(purpose, page_visible)")
            .unwrap()
            < replies.find("EditorSession::new(detail)").unwrap(),
        "tasks/replies.rs"
    );
    let today = include_str!("today.rs");
    assert!(
        today.find("complete_today(").unwrap()
            < today.find("self.emit_ui(UiEffect::RefreshToday)").unwrap(),
        "tasks/replies/today.rs"
    );
    assert!(
        replies.contains("if should_refresh(changed.today, page_visible)"),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("if should_refresh(changed.detail, page_visible)"),
        "tasks/replies.rs"
    );
    assert!(replies.contains("stage_confirmation("), "tasks/replies.rs");
    assert!(
        replies.contains("status_confirmation = previous"),
        "tasks/replies.rs"
    );
    assert!(
        replies.contains("delete_confirmation = previous"),
        "tasks/replies.rs"
    );
}

#[test]
fn test_save_failure_continuations_do_not_drop_navigation_or_build_a_main_window() {
    let transitions = include_str!("../transitions.rs");
    assert!(
        transitions.contains("self.dispatch_action(action);"),
        "tasks/transitions.rs"
    );
    assert!(
        transitions.contains("if minimize && self.with_ui(|_| ()).is_some()"),
        "tasks/transitions.rs"
    );
    assert!(
        transitions.contains("self.emit_ui(UiEffect::Dialog("),
        "tasks/transitions.rs"
    );
    assert!(
        transitions.contains("discard_exit_allowed = previous"),
        "tasks/transitions.rs"
    );
    for (file, source) in [
        ("tasks/replies.rs", include_str!("../replies.rs")),
        ("tasks/replies/failures.rs", include_str!("failures.rs")),
        ("tasks/replies/today.rs", include_str!("today.rs")),
    ] {
        for factory in [
            "MainWindow::new",
            "ShellHandle::new",
            "UiSession::new",
            "ensure_ui(",
        ] {
            assert!(!source.contains(factory), "{file}: {factory}");
        }
    }
}
