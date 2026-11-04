use fastnote_core::{Clock, NaiveDate};
use fastnote_ui::DialogKind;

use crate::controller::AppController;

use super::*;

struct FixedClock(NaiveDate);

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.0
    }
}

fn confirmation(command: &str, detail: &str) -> UiEffect {
    UiEffect::Dialog(
        DialogRequest::new(
            "仍要继续？",
            "操作尚未完成。",
            "继续",
            DialogKind::Destructive,
        )
        .with_command(command)
        .with_detail(detail),
    )
}

#[test]
fn test_same_confirmation_updates_in_place_but_different_commands_keep_fifo_order() {
    let mut pending = PendingEffects::default();
    assert!(pending.push(confirmation("desktop:force-quit", "fake-log-1")));
    assert!(pending.push(confirmation("task:reload-confirm:fake", "")));
    assert!(pending.push(confirmation("desktop:force-quit", "fake-log-2")));
    assert!(pending.has_dialog());
    assert_eq!(
        pending.entries,
        VecDeque::from([
            confirmation("desktop:force-quit", "fake-log-2"),
            confirmation("task:reload-confirm:fake", ""),
        ])
    );
}

#[test]
fn test_confirmation_without_command_uses_title_and_kind_as_its_key() {
    let mut pending = PendingEffects::default();
    let first = DialogRequest::new("第一个确认？", "说明", "继续", DialogKind::Normal);
    let second = DialogRequest::new("第二个确认？", "说明", "继续", DialogKind::Normal);
    let destructive = DialogRequest::new("第一个确认？", "说明", "继续", DialogKind::Destructive);
    for request in [
        first.clone(),
        second.clone(),
        destructive.clone(),
        first.clone(),
    ] {
        assert!(pending.push(UiEffect::Dialog(request)));
    }
    assert_eq!(
        pending.entries,
        VecDeque::from([
            UiEffect::Dialog(first),
            UiEffect::Dialog(second),
            UiEffect::Dialog(destructive),
        ])
    );
}

#[test]
fn test_confirmation_capacity_rejects_new_request_without_losing_existing_decisions() {
    let pending = RefCell::new(PendingEffects::default());
    for index in 0..MAX_PENDING_DIALOGS {
        assert!(pending
            .borrow_mut()
            .push(confirmation(&format!("fake:confirm:{index}"), "")));
    }
    let original = pending.borrow().entries.clone();
    assert!(!dispatch_effect(
        &pending,
        confirmation("fake:extra", ""),
        |_| false
    ));
    assert_eq!(pending.borrow().entries, original);
    assert!(pending.borrow().has_dialog());
    assert!(dispatch_effect(
        &pending,
        confirmation("fake:confirm:0", "new"),
        |_| false
    ));
    assert_eq!(pending.borrow().entries.len(), MAX_PENDING_DIALOGS);
    assert_eq!(
        pending.borrow().entries.front(),
        Some(&confirmation("fake:confirm:0", "new"))
    );
}

#[test]
fn test_absent_session_confirmation_is_queued_without_executing_its_command() {
    let pending = RefCell::new(PendingEffects::default());
    let presented = std::cell::Cell::new(false);
    assert!(dispatch_effect(
        &pending,
        confirmation("desktop:force-quit", "fake-log"),
        |_| false
    ));
    replay_effects(&pending, |_| {
        presented.set(true);
        false
    });
    assert!(presented.get());
    assert!(pending.borrow().has_dialog());
    assert_eq!(pending.borrow().entries.len(), 1);
    // 回放只把请求交给呈现端，没有命令执行器或窗口工厂。
    let delivered = RefCell::new(Vec::new());
    replay_effects(&pending, |effect| {
        delivered.borrow_mut().push(effect.clone());
        true
    });
    assert_eq!(
        delivered.borrow().as_slice(),
        &[confirmation("desktop:force-quit", "fake-log")]
    );
    assert!(!pending.borrow().has_dialog());
}

#[test]
fn test_latest_navigation_is_coalesced_without_discarding_confirmation_or_busy_state() {
    let mut pending = PendingEffects::default();
    pending.push(UiEffect::Navigate(
        "navigate:日历?selected=2026-10-23".to_owned(),
    ));
    pending.push(confirmation("desktop:force-quit", ""));
    pending.push(UiEffect::SetBusy(false));
    pending.push(UiEffect::Navigate(
        "navigate:设置?section=hotkeys".to_owned(),
    ));
    assert_eq!(
        pending.entries,
        VecDeque::from([
            UiEffect::Navigate("navigate:设置?section=hotkeys".to_owned()),
            confirmation("desktop:force-quit", ""),
            UiEffect::SetBusy(false),
        ])
    );
}

#[test]
fn test_route_change_clears_only_previous_route_toasts_and_keeps_confirmation_and_state() {
    let mut pending = PendingEffects::default();
    pending.push(UiEffect::SetDesktopState(2));
    pending.push(UiEffect::Toast(ToastRequest {
        kind: fastnote_ui::ToastKind::Info,
        message: "旧页提示".to_owned(),
        action: None,
    }));
    pending.push(confirmation("desktop:force-quit", ""));
    pending.discard_route_toasts();
    assert_eq!(
        pending.entries,
        VecDeque::from([
            UiEffect::SetDesktopState(2),
            confirmation("desktop:force-quit", "")
        ])
    );
    pending.clear();
    assert!(!pending.has_dialog());
}

#[test]
fn test_navigation_projection_rejects_stale_route_and_retains_current_parameters() {
    let date = "2026-11-04".parse().unwrap();
    let mut controller = AppController::new(FixedClock(date));
    let old = controller
        .handle_action("navigate:日历?selected=2026-10-23")
        .view;
    let new = controller
        .handle_action("navigate:日历?selected=2026-11-04")
        .view;
    assert!(!is_current_navigation(&old.route_debug, &new));
    assert!(is_current_navigation(&new.route_debug, &new));
    assert_eq!(controller.shell_view(), new);
}

#[test]
fn test_entry_and_close_paths_keep_kernel_state_before_presenting_effects() {
    let ball = include_str!("../ball.rs");
    assert!(ball.contains("self.entry.set(Some(mode));"), "ball.rs");
    assert!(ball.contains("self.sync_entry_availability();"), "ball.rs");
    assert!(!ball.contains("set_entry_available("), "ball.rs");
    let close = include_str!("../close.rs");
    assert!(close.contains("if self.dialog_blocked()"), "close.rs");
    assert!(
        close.contains("restore_if_present(self.with_ui(|_| ()).is_some()"),
        "close.rs"
    );
    assert!(
        close.contains("if self.emit_ui(UiEffect::Dialog(request))"),
        "close.rs"
    );
    let effects = include_str!("../effects.rs");
    assert!(
        effects.contains("window.set_entry_available(self.entry.get().is_some());"),
        "effects.rs"
    );
    let apply = effects.split_once("fn apply_ui_effect(").unwrap().1;
    assert!(!apply.contains("dispatch_action("), "effects.rs");
    assert!(!apply.contains("handle_action("), "effects.rs");
    assert!(!apply.contains("confirm_command"), "effects.rs");
}
