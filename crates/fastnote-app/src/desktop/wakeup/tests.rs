use std::cell::Cell;

use super::*;

#[test]
fn test_registered_kernel_receives_wakeup_without_any_ui_resource() {
    let target = Rc::new(Cell::new(0));
    let slot = RefCell::new(Rc::downgrade(&target));
    deliver(&slot, |target| target.set(target.get() + 1));
    assert_eq!(target.get(), 1);
    assert_eq!(Rc::strong_count(&target), 1);
}

#[test]
fn test_missing_or_dropped_kernel_wakeup_is_a_safe_noop() {
    let slot = RefCell::new(Weak::<Cell<i32>>::new());
    let called = Cell::new(false);
    deliver(&slot, |_| called.set(true));
    let target = Rc::new(Cell::new(0));
    *slot.borrow_mut() = Rc::downgrade(&target);
    drop(target);
    deliver(&slot, |_| called.set(true));
    assert!(!called.get());
}

#[test]
fn test_wakeup_handler_can_unregister_itself_without_a_refcell_conflict() {
    let target = Rc::new(Cell::new(0));
    let slot = RefCell::new(Rc::downgrade(&target));
    deliver(&slot, |target| {
        target.set(1);
        clear_matching(&slot, target.as_ref());
    });
    assert_eq!(target.get(), 1);
    assert!(slot.borrow().upgrade().is_none());
}

#[test]
fn test_cleanup_of_another_kernel_does_not_clear_the_current_registration() {
    let target = Rc::new(Cell::new(0));
    let other = Rc::new(Cell::new(1));
    let slot = RefCell::new(Rc::downgrade(&target));
    clear_matching(&slot, other.as_ref());
    assert!(slot.borrow().upgrade().is_some());
    clear_matching(&slot, target.as_ref());
    assert!(slot.borrow().upgrade().is_none());
}

#[test]
fn test_wakeup_factory_is_send_sync_without_initializing_ui_or_spawning_threads() {
    fn send_sync<T: Send + Sync>(_: &T) {}
    let wake = make_wakeup();
    send_sync(&wake);
    assert_eq!(Arc::strong_count(&wake), 1);
}

#[test]
fn test_production_wakeup_uses_event_loop_and_kernel_weak_reference_not_main_window() {
    let source = include_str!("../wakeup.rs");
    assert!(source.contains("slint::invoke_from_event_loop(dispatch)"));
    assert!(source.contains("RefCell<Weak<Runtime>>"));
    assert!(!source.contains(".as_weak()"));
    assert!(!source.contains("upgrade_in_event_loop"));
    assert!(!source.contains("thread::spawn"));
    let runtime = include_str!("../runtime.rs");
    assert!(runtime.contains("let wake = make_wakeup();"));
    assert!(runtime.contains("install_event_handlers(native_tx.clone(), wake.clone())"));
    assert!(include_str!("../lifecycle.rs").contains("super::wakeup::unregister(self);"));
}

#[test]
fn test_main_callbacks_are_bound_only_once_per_existing_session() {
    let source = include_str!("../window/ui_events.rs");
    let compact = source.split_whitespace().collect::<String>();
    assert!(
        compact.find("self.with_ui(|ui|").unwrap()
            < compact.find("window.on_desktop_wakeup(").unwrap()
    );
    assert!(source.contains("ui.callbacks_connected.replace(true)"));
    assert!(source.contains("if connected {"));
    assert!(source.contains("self.connect_tasks();"));
    assert!(!source.contains("MainWindow::new"));
    assert!(!source.contains("ShellHandle::new"));
    let kernel = include_str!("../window.rs")
        .split_once("fn connect_kernel(")
        .unwrap()
        .1
        .split_once("fn main_content_visible(")
        .unwrap()
        .0;
    assert!(!kernel.contains("self.with_ui("));
}
