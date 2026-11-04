use std::cell::Cell;

use fastnote_ui::{ToastAction, ToastKind};

use crate::desktop::ui_session::with_session;

use super::*;

fn toast(message: &str) -> UiEffect {
    UiEffect::Toast(ToastRequest {
        kind: ToastKind::Info,
        message: message.to_owned(),
        action: None,
    })
}

fn action_toast(label: &str, command: &str) -> UiEffect {
    UiEffect::Toast(ToastRequest {
        kind: ToastKind::Info,
        message: "请通过入口重试。".to_owned(),
        action: Some(ToastAction {
            label: label.to_owned(),
            command: command.to_owned(),
        }),
    })
}

#[test]
fn test_empty_effect_queue_does_not_attempt_presentation() {
    let pending = RefCell::new(PendingEffects::default());
    let calls = Cell::new(0);
    replay_effects(&pending, |_| {
        calls.set(calls.get() + 1);
        true
    });
    assert_eq!(calls.get(), 0);
    assert!(pending.borrow().entries.is_empty());
}

#[test]
fn test_absent_session_queues_effect_without_calling_projection_or_creating_ui() {
    let ui = RefCell::new(None::<u8>);
    let pending = RefCell::new(PendingEffects::default());
    let projected = Cell::new(false);
    let effect = UiEffect::SetDesktopState(1);
    dispatch_effect(&pending, effect.clone(), |_| {
        with_session(&ui, |_| projected.set(true)).is_some()
    });
    assert!(!projected.get());
    assert_eq!(*ui.borrow(), None);
    assert_eq!(pending.borrow().entries, VecDeque::from([effect]));
}

#[test]
fn test_existing_session_presents_effect_without_adding_pending_work() {
    let ui = RefCell::new(Some(Cell::new(0_u8)));
    let pending = RefCell::new(PendingEffects::default());
    dispatch_effect(&pending, UiEffect::SetDesktopState(1), |effect| {
        with_session(&ui, |session| {
            if let UiEffect::SetDesktopState(state) = effect {
                session.set(*state);
            }
        })
        .is_some()
    });
    assert_eq!(ui.borrow().as_ref().unwrap().get(), 1);
    assert!(pending.borrow().entries.is_empty());
}

#[test]
fn test_repeated_state_busy_and_focus_intents_keep_only_the_latest_values() {
    let mut pending = PendingEffects::default();
    for _ in 0..1_000 {
        pending.push(UiEffect::SetDesktopState(0));
        pending.push(UiEffect::SetBusy(true));
        pending.push(UiEffect::Focus(FocusTarget::Main));
        pending.push(UiEffect::SetDesktopState(1));
        pending.push(UiEffect::SetBusy(false));
    }
    assert_eq!(
        pending.entries,
        VecDeque::from([
            UiEffect::SetDesktopState(1),
            UiEffect::SetBusy(false),
            UiEffect::Focus(FocusTarget::Main),
        ])
    );
}

#[test]
fn test_same_toast_key_updates_action_label_in_place() {
    let mut pending = PendingEffects::default();
    pending.push(action_toast("查看", "navigate:设置"));
    pending.push(UiEffect::SetDesktopState(1));
    pending.push(action_toast("重试", "navigate:设置"));
    assert_eq!(
        pending.entries,
        VecDeque::from([
            action_toast("重试", "navigate:设置"),
            UiEffect::SetDesktopState(1),
        ])
    );
}

#[test]
fn test_different_toast_kind_message_or_command_is_not_merged() {
    let mut pending = PendingEffects::default();
    let info = toast("操作未完成。");
    let error = UiEffect::Toast(ToastRequest {
        kind: ToastKind::Error,
        message: "操作未完成。".to_owned(),
        action: None,
    });
    let effects = [
        info,
        error,
        toast("请稍后重试。"),
        action_toast("查看", "navigate:设置"),
        action_toast("查看", "navigate:今日任务"),
        toast("请通过入口重试。"),
    ];
    for effect in &effects {
        pending.push(effect.clone());
    }
    assert_eq!(pending.entries, VecDeque::from(effects));
}

#[test]
fn test_toast_capacity_replaces_oldest_toast_without_losing_state_busy_or_focus() {
    let mut pending = PendingEffects::default();
    let controls = [
        UiEffect::SetDesktopState(2),
        UiEffect::SetBusy(false),
        UiEffect::Focus(FocusTarget::Main),
    ];
    for effect in &controls {
        pending.push(effect.clone());
    }
    for index in 0..MAX_PENDING_TOASTS + 8 {
        pending.push(toast(&format!("提示 {index}")));
    }
    assert_eq!(pending.entries.len(), MAX_PENDING_TOASTS + controls.len());
    assert_eq!(
        pending.entries.iter().take(3).cloned().collect::<Vec<_>>(),
        controls
    );
    assert_eq!(pending.entries.get(3), Some(&toast("提示 8")));
    assert_eq!(
        pending.entries.back(),
        Some(&toast(&format!("提示 {}", MAX_PENDING_TOASTS + 7)))
    );
}

#[test]
fn test_duplicate_toast_at_capacity_updates_without_evicting_another_toast() {
    let mut pending = PendingEffects::default();
    for index in 0..MAX_PENDING_TOASTS {
        pending.push(toast(&format!("提示 {index}")));
    }
    pending.push(toast("提示 0"));
    assert_eq!(pending.entries.len(), MAX_PENDING_TOASTS);
    assert_eq!(pending.entries.front(), Some(&toast("提示 0")));
    assert_eq!(pending.entries.get(1), Some(&toast("提示 1")));
}

#[test]
fn test_absent_session_replay_stops_after_one_attempt_and_preserves_the_queue() {
    let pending = RefCell::new(PendingEffects::default());
    pending.borrow_mut().push(UiEffect::SetDesktopState(2));
    pending.borrow_mut().push(toast("暂时不可用。"));
    let original = pending.borrow().entries.clone();
    let attempts = Cell::new(0);
    replay_effects(&pending, |_| {
        attempts.set(attempts.get() + 1);
        false
    });
    assert_eq!(attempts.get(), 1);
    assert_eq!(pending.borrow().entries, original);
}

#[test]
fn test_replay_presents_pending_effects_once_in_fifo_order() {
    let pending = RefCell::new(PendingEffects::default());
    pending.borrow_mut().push(UiEffect::SetDesktopState(1));
    pending.borrow_mut().push(toast("入口已切换。"));
    let expected = pending.borrow().entries.clone();
    let presented = RefCell::new(VecDeque::new());
    replay_effects(&pending, |effect| {
        presented.borrow_mut().push_back(effect.clone());
        true
    });
    assert_eq!(*presented.borrow(), expected);
    assert!(pending.borrow().entries.is_empty());
    replay_effects(&pending, |_| panic!("effects must not be presented twice"));
}

#[test]
fn test_replay_does_not_hold_queue_borrow_across_reentrant_callbacks() {
    let pending = RefCell::new(PendingEffects::default());
    pending.borrow_mut().push(UiEffect::SetDesktopState(1));
    let presented = RefCell::new(Vec::new());
    replay_effects(&pending, |effect| {
        presented.borrow_mut().push(effect.clone());
        if matches!(effect, UiEffect::SetDesktopState(_)) {
            dispatch_effect(&pending, UiEffect::SetBusy(false), |_| false);
        }
        true
    });
    assert_eq!(
        *presented.borrow(),
        [UiEffect::SetDesktopState(1), UiEffect::SetBusy(false)]
    );
    assert!(pending.borrow().entries.is_empty());
}

#[test]
fn test_clear_discards_pending_feedback_without_presenting_it() {
    let mut pending = PendingEffects::default();
    pending.push(UiEffect::SetDesktopState(2));
    pending.push(toast("暂时不可用。"));
    pending.clear();
    assert!(pending.entries.is_empty());
}

#[test]
fn test_latest_startup_failure_is_retained_until_success_and_keeps_hotkey_status() {
    let mut feedback = DesktopFeedback::default();
    assert_eq!(feedback.state, 0);
    assert_eq!(feedback.hotkey_status, "正在注册…");
    feedback.hotkey_status = "未设置（未注册）".to_owned();
    feedback.storage_failed("无法读取数据。", "fake-log-1");
    feedback.storage_failed("数据暂不可用。", "fake-log-2");
    assert_eq!(feedback.state, 2);
    assert_eq!(feedback.storage_message, "数据暂不可用。");
    assert_eq!(feedback.storage_log_id, "fake-log-2");
    feedback.set_state(0);
    assert_eq!(feedback.state, 0);
    assert_eq!(feedback.storage_log_id, "fake-log-2");
    feedback.set_state(1);
    assert_eq!(feedback.state, 1);
    assert!(feedback.storage_message.is_empty());
    assert!(feedback.storage_log_id.is_empty());
    assert_eq!(feedback.hotkey_status, "未设置（未注册）");
}

#[test]
fn test_startup_replies_and_presentation_use_effects_without_creating_windows() {
    // 只检查接线；真实窗口、焦点及系统入口由用户手工回归。
    let lifecycle = include_str!("../lifecycle.rs");
    assert!(
        lifecycle.contains("self.set_desktop_state(1);"),
        "lifecycle.rs"
    );
    assert!(
        lifecycle.contains("self.set_storage_error(message, error.log_id());"),
        "lifecycle.rs"
    );
    assert!(
        lifecycle.contains("self.set_hotkey_status(status);"),
        "lifecycle.rs"
    );
    assert!(
        lifecycle.contains("self.pending_effects.borrow_mut().clear();"),
        "lifecycle.rs"
    );
    assert!(
        !lifecycle.contains("set_storage_error_message("),
        "lifecycle.rs"
    );
    let presentation = include_str!("../presentation/mod.rs");
    assert!(
        presentation.contains("self.emit_ui(UiEffect::Focus(FocusTarget::Main));"),
        "presentation/mod.rs"
    );
    assert!(
        presentation.find("self.flush_ui_effects();").unwrap()
            < presentation.find("ui.shell.show()").unwrap(),
        "presentation/mod.rs"
    );
    let effects = include_str!("../effects.rs");
    for factory in ["ShellHandle::new", "MainWindow::new", "UiSession::new"] {
        assert!(!effects.contains(factory), "effects.rs: {factory}");
        assert!(
            !presentation.contains(factory),
            "presentation/mod.rs: {factory}"
        );
    }
}

#[test]
fn test_today_projection_intents_are_coalesced_without_replacing_confirmation() {
    let mut pending = PendingEffects::default();
    pending.push(UiEffect::RefreshToday);
    pending.push(UiEffect::Dialog(
        fastnote_ui::DialogRequest::new(
            "继续修改？",
            "操作需要确认。",
            "继续",
            fastnote_ui::DialogKind::Normal,
        )
        .with_command("task:status-confirm:fake"),
    ));
    for _ in 0..1_000 {
        pending.push(UiEffect::RefreshToday);
    }
    assert_eq!(pending.entries.len(), 2);
    assert_eq!(pending.entries.front(), Some(&UiEffect::RefreshToday));
    assert!(pending.has_dialog());
}

#[test]
fn test_no_ui_focus_intent_is_kept_without_invoking_a_component() {
    let pending = RefCell::new(PendingEffects::default());
    let ui = RefCell::new(None::<u8>);
    let invoked = Cell::new(false);
    assert!(dispatch_effect(
        &pending,
        UiEffect::Focus(FocusTarget::TaskCreate),
        |_| { with_session(&ui, |_| invoked.set(true)).is_some() }
    ));
    assert!(!invoked.get());
    assert_eq!(*ui.borrow(), None);
    assert_eq!(
        pending.borrow().entries,
        VecDeque::from([UiEffect::Focus(FocusTarget::TaskCreate)])
    );
}

#[test]
fn test_today_effect_is_projection_only_and_does_not_query_or_create_ui() {
    let effects = include_str!("../effects.rs");
    let projection = effects
        .split_once("UiEffect::RefreshToday => {")
        .unwrap()
        .1
        .split_once("UiEffect::SetDesktopState(")
        .unwrap()
        .0;
    assert!(
        projection.contains("if !self.today_dirty.get()"),
        "effects.rs"
    );
    assert!(projection.contains("self.apply_today();"), "effects.rs");
    assert!(!projection.contains("request_today("), "effects.rs");
    assert!(!projection.contains("ensure_ui("), "effects.rs");
}
