use std::cell::{Cell, RefCell};

use super::*;
use crate::desktop::lease::{UiLease, HOT_LEASE_MS};
use crate::tasks::UiEpoch;

fn sample(working_set_bytes: u64, system_load_percent: u32) -> ProcessResources {
    ProcessResources {
        working_set_bytes,
        private_bytes: 0,
        handles: 0,
        gdi_objects: 0,
        user_objects: 0,
        system_load_percent,
        available_physical_bytes: 0,
        total_physical_bytes: 0,
    }
}

#[test]
fn test_budget_uses_decimal_working_set_not_private_bytes_or_mib() {
    assert_eq!(pressure_reason(sample(HOT_BUDGET_BYTES, 50)), None);
    assert_eq!(
        pressure_reason(sample(HOT_BUDGET_BYTES + 1, 50)),
        Some(ReleaseReason::Budget)
    );
    let mut value = sample(10_000_000, 50);
    value.private_bytes = 500_000_000;
    assert_eq!(pressure_reason(value), None);
    assert_eq!(COLD_BUDGET_BYTES, 80_000_000);
}

#[test]
fn test_windows_system_pressure_is_an_early_eviction_signal_at_the_boundary() {
    assert_eq!(
        pressure_reason(sample(20_000_000, SYSTEM_PRESSURE_PERCENT - 1)),
        None
    );
    assert_eq!(
        pressure_reason(sample(20_000_000, SYSTEM_PRESSURE_PERCENT)),
        Some(ReleaseReason::Pressure)
    );
    assert_eq!(
        pressure_reason(sample(200_000_000, SYSTEM_PRESSURE_PERCENT)),
        Some(ReleaseReason::Pressure)
    );
}

struct Ticks(Cell<u64>);
impl TickSource for Ticks {
    fn milliseconds(&self) -> u64 {
        self.0.get()
    }
}

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn test_five_hot_expiry_cycles_drop_each_ui_and_keep_kernel_and_weak_ownership_stable() {
    let clock = Ticks(Cell::new(0));
    let dropped = Rc::new(Cell::new(0));
    let kernel = Rc::new(Cell::new(0));
    let slot = RefCell::new(None::<Probe>);
    let mut lease = UiLease::default();
    let mut epoch = UiEpoch::default();
    for cycle in 0..5 {
        assert!(slot.borrow().is_none());
        assert!(epoch.mount());
        *slot.borrow_mut() = Some(Probe(dropped.clone()));
        lease.apply(LeaseEvent::Mounted, &clock);
        let generation = match lease.apply(LeaseEvent::Hidden { protected: false }, &clock) {
            LeaseAction::Arm { generation, .. } => generation,
            other => panic!("expected armed lease: {other:?}"),
        };
        clock.0.set(clock.0.get() + HOT_LEASE_MS);
        assert_eq!(
            lease.apply(LeaseEvent::Tick { generation }, &clock),
            LeaseAction::Release(ReleaseReason::Expired)
        );
        epoch.unmount();
        drop(slot.borrow_mut().take());
        assert_eq!(dropped.get(), cycle + 1);
        assert_eq!(Rc::strong_count(&dropped), 1);
        assert_eq!(Rc::strong_count(&kernel), 1);
        kernel.set(kernel.get() + 1);
    }
    assert_eq!(kernel.get(), 5);
}

#[test]
fn test_real_release_is_take_and_drop_not_hide_and_preserves_kernel_timers_and_worker() {
    let source = include_str!("../resources.rs");
    let release = source
        .split_once("fn release_ui(")
        .unwrap()
        .1
        .split_once("fn failed_ui_show(")
        .unwrap()
        .0;
    assert!(release.contains("self.ui.borrow_mut().take()"));
    assert!(release.contains("drop(ui)"));
    assert!(release.contains("epoch.unmount()"));
    assert!(release.contains("as_weak()"));
    assert!(!release.contains("task_day_timer.stop()"));
    assert!(!release.contains("worker"));
    assert!(!release.contains("ball.hide()"));
    assert!(!release.contains("menu.hide()"));
    assert!(!source.contains("EmptyWorkingSet"));
    assert!(!source.contains("SetProcessWorkingSetSize"));
}

#[test]
fn test_expiry_is_checked_before_replies_and_reopen_can_use_the_old_session() {
    let runtime = include_str!("../runtime.rs");
    let wakeup = runtime
        .split_once("fn wakeup(")
        .unwrap()
        .1
        .split_once("fn set_busy(")
        .unwrap()
        .0;
    assert!(
        wakeup.find("self.check_ui_lease()").unwrap()
            < wakeup.find("self.worker.try_reply()").unwrap()
    );
    let session = include_str!("../runtime/session.rs");
    assert!(
        session.find("self.check_ui_lease()").unwrap()
            < session.find("ensure_session(&self.ui").unwrap()
    );
    let platform = include_str!("../../../../fastnote-platform/src/windows/resources.rs");
    assert!(platform.contains("GetTickCount64()"));
    assert!(!platform.contains("Instant"));
}

#[test]
fn test_real_hot_entry_waits_for_save_and_successful_hide_and_uses_backend_teardown() {
    let close = include_str!("../close.rs");
    let complete = close
        .split_once("fn complete_close(")
        .unwrap()
        .1
        .split_once("fn close_changes(")
        .unwrap()
        .0;
    assert!(
        complete.find("self.hide_to_entry().is_err()").unwrap()
            < complete.find("self.start_hot_lease()").unwrap()
    );
    let saved = include_str!("../lifecycle.rs");
    let saved = saved
        .split_once("fn saved(")
        .unwrap()
        .1
        .split_once("fn finish_exit(")
        .unwrap()
        .0;
    assert!(
        saved.find("if let Err(error) = result").unwrap()
            < saved.find("self.complete_close(exit").unwrap()
    );
    let backend = include_str!("../mod.rs");
    assert!(backend.contains("std::env::set_var(\"SLINT_DESTROY_WINDOW_ON_HIDE\", \"1\")"));
    let events = include_str!("../window/ui_events.rs");
    assert!(events.contains("RenderingState::RenderingTeardown"));
    assert!(events.contains("set_rendering_notifier"));
    let present = include_str!("../presentation/mod.rs");
    let hide = present.split_once("impl Surfaces for Runtime").unwrap().1;
    let hide = hide.split_whitespace().collect::<String>();
    assert!(hide.find("ui.shell.hide()").unwrap() < hide.find("ui_hidden_at.set(Some(").unwrap());
    let resource = include_str!("../resources.rs");
    let enter = resource
        .split_once("fn start_hot_lease(")
        .unwrap()
        .1
        .split_once("fn ui_shown(")
        .unwrap()
        .0;
    assert!(enter.contains("&HiddenAt(hidden_at)"));
}

#[test]
fn test_page_stamps_gate_both_success_and_failure_replies_and_callbacks_are_weak() {
    let requests = include_str!("../tasks/requests.rs");
    assert!(requests.contains("tasks.read_stamps.insert(id, stamp)"));
    assert!(requests.contains("session_generation = stamp.session_generation"));
    let detail = include_str!("../tasks/replies.rs");
    let detail = detail.split_once("fn task_queried(").unwrap().1;
    assert!(
        detail.find("read_stamps.remove(&id)").unwrap() < detail.find("latest_open != id").unwrap()
    );
    assert!(
        detail.find("ui_epoch.get().accepts(stamp)").unwrap()
            < detail.find("let detail = match result").unwrap()
    );
    let today = include_str!("../tasks/replies/today.rs");
    assert!(
        today.find("ui_epoch.get().accepts(stamp)").unwrap()
            < today.find("let (view, error) = match result").unwrap()
    );
    let callbacks = include_str!("callbacks.rs");
    assert!(callbacks.contains("Weak<Runtime>"));
    assert!(callbacks.contains("ui_epoch.get().accepts(stamp)"));
    assert!(!callbacks.contains("MainWindow"));
}
