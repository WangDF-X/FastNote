use std::cell::Cell;

use fastnote_ui::DialogKind;

use crate::desktop::ui_session::with_session;

use super::*;

#[test]
fn test_no_ui_editor_focus_is_buffered_without_calling_a_component() {
    let id = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    let ui = RefCell::new(None::<u8>);
    let pending = RefCell::new(PendingEffects::default());
    let called = Cell::new(false);
    assert!(dispatch_effect(
        &pending,
        UiEffect::Focus(FocusTarget::TaskEditor(id)),
        |_| { with_session(&ui, |_| called.set(true)).is_some() }
    ));
    assert!(!called.get());
    assert_eq!(*ui.borrow(), None);
    assert_eq!(
        pending.borrow().entries,
        VecDeque::from([UiEffect::Focus(FocusTarget::TaskEditor(id))])
    );
}

#[test]
fn test_latest_task_focus_replaces_old_target_without_discarding_confirmation() {
    let first = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    let second = "019a0fff-aaaa-7000-8000-000000000002".parse().unwrap();
    let mut pending = PendingEffects::default();
    pending.push(UiEffect::Focus(FocusTarget::TaskEditor(first)));
    pending.push(UiEffect::Dialog(
        DialogRequest::new(
            "重新加载？",
            "假输入需要确认。",
            "加载",
            DialogKind::Destructive,
        )
        .with_command("task:reload-confirm:fake"),
    ));
    for _ in 0..1_000 {
        pending.push(UiEffect::Focus(FocusTarget::TaskRow(second)));
    }
    assert_eq!(pending.entries.len(), 2);
    assert_eq!(
        pending.entries.front(),
        Some(&UiEffect::Focus(FocusTarget::TaskRow(second)))
    );
    assert!(pending.has_dialog());
}
