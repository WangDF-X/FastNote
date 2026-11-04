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
