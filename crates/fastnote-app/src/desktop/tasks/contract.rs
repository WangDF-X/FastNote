fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap()
        .1
        .split_once(end)
        .unwrap()
        .0
}

#[test]
fn test_no_session_query_gate_precedes_id_allocation_and_worker_calls() {
    let requests = include_str!("requests.rs");
    let today = between(requests, "fn request_today(", "fn request_task_detail(");
    assert!(
        today.contains("self.today_dirty.set(true);"),
        "tasks/requests.rs"
    );
    assert!(
        today.find("!may_query(None,").unwrap() < today.find("self.next_id.get()").unwrap(),
        "tasks/requests.rs"
    );
    assert!(
        today.find("!may_query(None,").unwrap() < today.find(".query(").unwrap(),
        "tasks/requests.rs"
    );
    let detail = between(requests, "fn request_task_detail(", "fn create_task(");
    assert!(
        detail.contains("self.detail_dirty.set(true);"),
        "tasks/requests.rs"
    );
    assert!(
        detail.find("!may_query(Some(purpose),").unwrap()
            < detail.find("self.next_id.get()").unwrap(),
        "tasks/requests.rs"
    );
    assert!(
        detail.find("!may_query(Some(purpose),").unwrap()
            < detail.find("self.worker.query(").unwrap(),
        "tasks/requests.rs"
    );
    assert!(
        detail.contains("save_read_failed(&mut self.tasks.borrow_mut(), purpose, message)"),
        "tasks/requests.rs"
    );
}

#[test]
fn test_optional_ui_access_precedes_expensive_editor_and_today_snapshot_clones() {
    let view = include_str!("view.rs");
    let editor = between(view, "fn sync_task_editor(", "fn apply_today(");
    assert!(
        editor.find("self.with_ui(|ui|").unwrap() < editor.find("editor.fields.clone()").unwrap(),
        "tasks/view.rs"
    );
    assert!(
        editor.contains("self.detail_dirty.set(true);"),
        "tasks/view.rs"
    );
    assert!(
        editor.contains("while_syncing(&self.task_syncing,"),
        "tasks/view.rs"
    );
    let today = between(view, "fn apply_today(", "fn restore_task_list_focus(");
    assert!(
        today.find("self.with_ui(|ui|").unwrap() < today.find("tasks.today.clone()").unwrap(),
        "tasks/view.rs"
    );
    assert!(
        today.contains("self.today_dirty.set(true);"),
        "tasks/view.rs"
    );
    assert!(!today.contains("request_today("), "tasks/view.rs");
}

#[test]
fn test_close_updates_kernel_before_ui_projection_and_keeps_task_focus_as_an_effect() {
    let view = include_str!("view.rs");
    let close = between(
        view,
        "fn close_task_editor(",
        "fn discard_task_editor_for_reload(",
    );
    assert!(
        close
            .find("close_editor(&mut self.tasks.borrow_mut())")
            .unwrap()
            < close.find("self.with_ui(|ui|").unwrap(),
        "tasks/view.rs"
    );
    assert!(close.contains("FocusTarget::TaskRow"), "tasks/view.rs");
    assert!(
        close.contains("self.emit_ui(UiEffect::Focus(target))"),
        "tasks/view.rs"
    );
}

#[test]
fn test_confirmation_dispatch_is_kernel_only_and_rolls_back_rejected_commands() {
    let source = include_str!("transitions/confirmations.rs");
    assert!(
        source.contains("self.confirmed_mutation_ready()"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains("self.task_kernel_available() && !self.tasks.borrow().unfinished()"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains("resolve_confirmation("),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains(".take_if("),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains("delete_confirmation = Some(previous)"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains("status_confirmation = Some(previous)"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        source.contains("self.perform_task_transition(Transition::DeleteConfirmed(*id))"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        !source.contains("self.can_task_interact()"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        !source.contains("self.task_transition("),
        "tasks/transitions/confirmations.rs"
    );
}

#[test]
fn test_reload_dialog_is_buffered_and_read_acceptance_precedes_draft_discard() {
    let source = include_str!("transitions/confirmations.rs");
    let reload = between(
        source,
        "fn request_editor_reload(",
        "fn handle_task_action(",
    );
    assert!(
        reload.contains("stage_confirmation("),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        reload.contains("self.emit_ui(UiEffect::Dialog("),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        reload.contains("reload_confirmation = previous"),
        "tasks/transitions/confirmations.rs"
    );
    let confirm = source
        .split_once("action.strip_prefix(\"task:reload-confirm:\")")
        .unwrap()
        .1;
    assert!(
        confirm.find("!may_query(").unwrap() < confirm.find(".reload_confirmation").unwrap(),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        confirm
            .find("self.request_task_detail(DetailPurpose::Open(*id))")
            .unwrap()
            < confirm
                .find("self.discard_task_editor_for_reload(*id)")
                .unwrap(),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        confirm.contains("reload_confirmation = Some(previous)"),
        "tasks/transitions/confirmations.rs"
    );
    assert!(
        !confirm.contains("self.close_task_editor()"),
        "tasks/transitions/confirmations.rs"
    );
}

#[test]
fn test_deferred_editor_focus_captures_target_and_effects_only_project_existing_page() {
    let requests = include_str!("requests.rs")
        .split_once("fn defer_task_focus(")
        .unwrap()
        .1;
    assert!(
        requests.find("let Some(id) = selected").unwrap()
            < requests.find("Timer::single_shot(").unwrap(),
        "tasks/requests.rs"
    );
    assert!(
        requests.contains("UiEffect::Focus(FocusTarget::TaskEditor(id))"),
        "tasks/requests.rs"
    );
    assert!(
        !requests.contains("invoke_focus_task_editor()"),
        "tasks/requests.rs"
    );
    let view = include_str!("view.rs");
    let focus = between(view, "fn focus_task_editor(", "fn focus_task_row(");
    assert!(focus.contains("!self.can_focus_tasks()"), "tasks/view.rs");
    assert!(focus.contains("editor_focus_matches("), "tasks/view.rs");
    let effects = include_str!("../effects.rs");
    assert!(
        effects.contains("self.focus_task_editor(*id)"),
        "effects.rs"
    );
    assert!(effects.contains("self.focus_task_row(*id)"), "effects.rs");
    assert!(!focus.contains("request_task_detail("), "tasks/view.rs");
}

#[test]
fn test_page_callbacks_guard_visibility_before_mutating_kernel_input_or_pagination() {
    for (file, source) in [
        ("tasks/autosave.rs", include_str!("autosave.rs")),
        ("tasks/datetime.rs", include_str!("datetime.rs")),
    ] {
        assert!(
            source.contains("!self.can_task_interact() || !self.can_focus_tasks()"),
            "{file}"
        );
    }
    let connect = include_str!("connect.rs");
    for (start, end) in [
        ("window.on_task_focus_move(", "window.on_task_edit("),
        ("window.on_today_section(", "window.on_today_more("),
    ] {
        let callback = between(connect, start, end);
        assert!(
            callback.contains("!runtime.can_task_interact() || !runtime.can_focus_tasks()"),
            "tasks/connect.rs: {start}"
        );
    }
    let more = connect.split_once("window.on_today_more(").unwrap().1;
    assert!(
        more.find("!runtime.can_focus_tasks()").unwrap() < more.find(".load_more(index)").unwrap(),
        "tasks/connect.rs"
    );
}

#[test]
fn test_p1_keeps_startup_construction_and_initial_query_until_p2() {
    let runtime = include_str!("../runtime.rs");
    assert!(
        runtime.contains("let shell = ShellHandle::new("),
        "runtime.rs"
    );
    assert!(runtime.contains("ui: RefCell::new(Some(ui))"), "runtime.rs");
    assert!(
        include_str!("../lifecycle.rs").contains("self.start_tasks();"),
        "lifecycle.rs"
    );
    let requests = between(
        include_str!("requests.rs"),
        "fn start_tasks(",
        "fn can_task_interact(",
    );
    assert!(
        requests.find("self.request_today();").unwrap()
            < requests.find("self.task_day_timer").unwrap(),
        "tasks/requests.rs"
    );
}

#[test]
fn test_request_view_and_transition_paths_never_construct_main_ui_resources() {
    for (file, source) in [
        ("tasks/requests.rs", include_str!("requests.rs")),
        ("tasks/view.rs", include_str!("view.rs")),
        ("tasks/transitions.rs", include_str!("transitions.rs")),
        (
            "tasks/transitions/confirmations.rs",
            include_str!("transitions/confirmations.rs"),
        ),
        ("tasks/policy.rs", include_str!("policy.rs")),
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
