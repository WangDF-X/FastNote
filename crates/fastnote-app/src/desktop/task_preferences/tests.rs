use super::*;

#[test]
fn test_sort_actions_are_typed_and_do_not_request_a_group_reveal() {
    for (value, sort, index) in [
        ("priority", TodaySort::Priority, 0),
        ("due_date", TodaySort::DueDate, 1),
        ("created_desc", TodaySort::CreatedDescending, 2),
    ] {
        let request = parse_request(&format!("settings:tasks:sort:{value}")).unwrap();
        assert_eq!(request.change, DesktopSetting::TodaySort(sort));
        assert!(!request.reveal_unscheduled);
        assert_eq!(sort_index(sort), index);
    }
}

#[test]
fn test_visibility_actions_only_accept_the_four_supported_groups_and_strict_boolean_values() {
    for (group, setting) in [
        (
            "in-progress",
            DesktopSetting::TodayShowInProgress as fn(bool) -> DesktopSetting,
        ),
        ("waiting", DesktopSetting::TodayShowWaiting),
        ("unscheduled", DesktopSetting::TodayShowUnscheduled),
        ("completed", DesktopSetting::TodayShowCompleted),
    ] {
        for shown in [false, true] {
            let request = parse_request(&format!("settings:tasks:{group}:{shown}")).unwrap();
            assert_eq!(request.change, setting(shown));
            assert!(!request.reveal_unscheduled);
        }
    }
    for invalid in [
        "navigate:设置?section=task-preferences",
        "settings:tasks:",
        "settings:tasks:sort:created",
        "settings:tasks:sort:priority:true",
        "settings:tasks:waiting:1",
        "settings:tasks:waiting:True",
        "settings:tasks:waiting:false:extra",
        "settings:tasks:overdue:false",
        "settings:tasks:tomorrow:false",
        "settings:tasks:due-today:false",
        "settings:tasks:reveal-unscheduled:false",
    ] {
        assert_eq!(parse_request(invalid), None, "{invalid}");
    }
}

#[test]
fn test_confirmed_value_reads_the_current_snapshot_and_rejects_other_settings() {
    let preferences = TodayPreferences {
        sort: TodaySort::CreatedDescending,
        show_in_progress: false,
        show_waiting: false,
        show_unscheduled: false,
        show_completed: false,
        ..Default::default()
    };
    for (requested, expected) in [
        (
            DesktopSetting::TodaySort(TodaySort::Priority),
            DesktopSetting::TodaySort(TodaySort::CreatedDescending),
        ),
        (
            DesktopSetting::TodayShowInProgress(true),
            DesktopSetting::TodayShowInProgress(false),
        ),
        (
            DesktopSetting::TodayShowWaiting(true),
            DesktopSetting::TodayShowWaiting(false),
        ),
        (
            DesktopSetting::TodayShowUnscheduled(true),
            DesktopSetting::TodayShowUnscheduled(false),
        ),
        (
            DesktopSetting::TodayShowCompleted(true),
            DesktopSetting::TodayShowCompleted(false),
        ),
    ] {
        assert_eq!(confirmed_value(preferences, &requested), Some(expected));
    }
    assert_eq!(
        confirmed_value(preferences, &DesktopSetting::FirstRunDone(true)),
        None
    );
}

#[test]
fn test_hidden_creation_notice_has_an_explicit_persistent_reveal_action_without_changing_preferences(
) {
    assert_eq!(hidden_task_created(TodayPreferences::default()), None);
    let preferences = TodayPreferences {
        show_unscheduled: false,
        show_waiting: false,
        ..Default::default()
    };
    let request = hidden_task_created(preferences).unwrap();
    assert_eq!(request.kind, ToastKind::Info);
    assert_eq!(request.message, "任务已添加，未安排分组已隐藏。");
    assert_eq!(request.kind.duration_ms(request.action.is_some()), 6000);
    let action = request.action.unwrap();
    assert_eq!(action.label, "撤销隐藏分组");
    let reveal = parse_request(&action.command).unwrap();
    assert_eq!(reveal.change, DesktopSetting::TodayShowUnscheduled(true));
    assert!(reveal.reveal_unscheduled);
    assert!(!preferences.show_unscheduled && !preferences.show_waiting);
}

#[test]
fn test_preference_save_failure_returns_before_applying_snapshot_and_success_requeries_today() {
    // 仅检查接线，真实事务由 store/worker 测试验证；不创建窗口。
    let lifecycle = include_str!("../lifecycle.rs");
    let failed = lifecycle
        .split_once("if let Err(error) = result {")
        .unwrap()
        .1;
    let (failed, applied) = failed
        .split_once("for change in &pending.changes {")
        .unwrap();
    assert!(
        failed.contains("Completion::TaskPreference { .. } => self.task_preference_failed(&error)")
    );
    assert!(failed.contains("return;"));
    assert!(applied.contains("self.settings.borrow_mut().apply(change);"));
    assert!(applied.contains("self.task_preference_committed(reveal_unscheduled);"));
    let source = include_str!("../task_preferences.rs");
    let success = source
        .split_once("pub(super) fn task_preference_committed(")
        .unwrap()
        .1;
    assert!(success.contains("self.request_today();"));
    assert!(success.contains("request_unscheduled_reveal();"));
    let replies = include_str!("../tasks/replies.rs");
    assert!(replies.contains("if preferences.show_unscheduled {"));
    assert!(replies.contains("self.shell.show_toast(request);"));
    assert!(replies.contains("take_unscheduled_reveal();"));
}

#[test]
fn test_modal_disabling_is_live_in_the_ui_and_does_not_stick_in_the_preference_snapshot() {
    let source = include_str!("../task_preferences.rs");
    let projection = source
        .split_once("pub(super) fn sync_task_preferences(&self) {")
        .unwrap()
        .1
        .split_once("pub(super) fn handle_task_preferences_action(")
        .unwrap()
        .0;
    assert!(!projection.contains("self.can_task_interact()"));
    assert!(!projection.contains("get_dialog_visible()"));
    assert!(projection.contains("!self.busy.get()"));
    assert!(source.contains("if !self.can_task_interact() {"));
    let main = include_str!("../../../../fastnote-ui/ui/app-window.slint");
    assert!(main.contains("enabled: root.task-preferences-enabled && !root.dialog-visible;"));
}
