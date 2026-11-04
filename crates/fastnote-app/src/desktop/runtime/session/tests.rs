use std::cell::Cell;

use fastnote_core::{Clock, NaiveDate};

use crate::controller::AppController;

use super::*;

struct FixedClock;
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        "2026-11-04".parse().unwrap()
    }
}

#[test]
fn test_missing_session_constructs_once_and_existing_session_is_reused() {
    let slot = RefCell::new(None::<u8>);
    let calls = Cell::new(0);
    assert_eq!(
        ensure_session(&slot, || {
            calls.set(calls.get() + 1);
            Ok::<_, ()>(7)
        }),
        Ok(true)
    );
    assert_eq!(
        ensure_session(&slot, || {
            calls.set(calls.get() + 1);
            Ok::<_, ()>(9)
        }),
        Ok(false)
    );
    assert_eq!(*slot.borrow(), Some(7));
    assert_eq!(calls.get(), 1);
}

#[test]
fn test_failed_construction_leaves_session_absent_and_allows_retry() {
    let slot = RefCell::new(None::<u8>);
    assert_eq!(
        ensure_session(&slot, || Err::<u8, _>("unavailable")),
        Err("unavailable")
    );
    assert_eq!(*slot.borrow(), None);
    assert_eq!(ensure_session(&slot, || Ok::<_, &str>(7)), Ok(true));
    assert_eq!(*slot.borrow(), Some(7));
}

#[test]
fn test_factory_does_not_hold_session_borrow_and_keeps_current_parameter_route() {
    let slot = RefCell::new(None::<String>);
    let controller = RefCell::new(AppController::new(FixedClock));
    controller
        .borrow_mut()
        .handle_action("navigate:日历?selected=2025-03-17");
    let before = controller.borrow().shell_view().route_debug;
    assert_eq!(
        ensure_session(&slot, || {
            assert!(slot.borrow().is_none());
            let route = controller.borrow().shell_view().route_debug;
            Ok::<_, ()>(route)
        }),
        Ok(true)
    );
    assert_eq!(slot.borrow().as_ref(), Some(&before));
    assert_eq!(controller.borrow().shell_view().route_debug, before);
}

#[test]
fn test_normal_hidden_startup_creates_no_main_session_until_explicit_ensure() {
    for hidden in [true, false] {
        let slot = RefCell::new(None::<u8>);
        let calls = Cell::new(0);
        if !hidden {
            ensure_session(&slot, || {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(7)
            })
            .unwrap();
        }
        assert_eq!(calls.get(), u32::from(!hidden));
        ensure_session(&slot, || {
            calls.set(calls.get() + 1);
            Ok::<_, ()>(7)
        })
        .unwrap();
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn test_first_show_uses_live_settings_feedback_and_current_route_without_reexecuting_actions() {
    let source = include_str!("../session.rs");
    assert!(
        source.contains("self.controller.borrow().shell_view()"),
        "runtime/session.rs"
    );
    for restore in [
        "self.sync_settings();",
        "self.sync_entry_availability();",
        "UiEffect::SetBusy(self.busy.get())",
        "self.flush_ui_effects();",
        "self.ensure_current_page_resources();",
    ] {
        assert!(source.contains(restore), "runtime/session.rs: {restore}");
    }
    let projection = source.split_once("if created {").unwrap().1;
    assert!(!projection.contains("handle_action("), "runtime/session.rs");
    assert!(!projection.contains(".dispatch("), "runtime/session.rs");
    let resources = source
        .split_once("fn ensure_current_page_resources(")
        .unwrap()
        .1
        .split_once("/// 不跨工厂借用")
        .unwrap()
        .0;
    assert!(
        resources.contains("!self.ready.get()"),
        "runtime/session.rs"
    );
    assert!(!resources.contains("request_today("), "runtime/session.rs");
}
