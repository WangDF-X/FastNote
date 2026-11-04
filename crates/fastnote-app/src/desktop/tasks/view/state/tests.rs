use std::cell::RefCell;

use fastnote_core::{Task, TaskContext, TaskDetail};

use crate::desktop::ui_session::with_session;
use crate::tasks::EditorSession;

use super::*;

fn id(number: usize) -> TaskId {
    format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .unwrap()
}

fn editor() -> EditorSession {
    let task = Task::new(
        id(1),
        "假任务".to_owned(),
        "2026-11-04T12:00:00Z".parse().unwrap(),
    )
    .unwrap();
    let mut editor = EditorSession::new(TaskDetail {
        task,
        context: TaskContext::default(),
        is_overdue: false,
        is_blocked: false,
    });
    editor.edit("title", "待保留的假输入".to_owned()).unwrap();
    editor
}

#[test]
fn test_projection_without_ui_does_not_build_or_clone_an_editor_snapshot() {
    let ui = RefCell::new(None::<()>);
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor());
    let cloned = Cell::new(false);
    let result = with_session(&ui, |_| {
        cloned.set(true);
        tasks.editor.as_ref().unwrap().fields.clone()
    });
    assert!(result.is_none());
    assert!(!cloned.get());
    assert_eq!(*ui.borrow(), None);
    assert_eq!(
        tasks.editor.as_ref().unwrap().fields.title,
        "待保留的假输入"
    );
}

#[test]
fn test_syncing_guard_restores_state_and_returns_projection_result() {
    let syncing = Cell::new(false);
    assert_eq!(
        while_syncing(&syncing, || {
            assert!(syncing.get());
            7
        }),
        7
    );
    assert!(!syncing.get());
}

#[test]
fn test_nested_projection_does_not_enable_reentrant_save_before_outer_projection_ends() {
    let syncing = Cell::new(false);
    while_syncing(&syncing, || {
        while_syncing(&syncing, || assert!(syncing.get()));
        assert!(syncing.get());
    });
    assert!(!syncing.get());
    syncing.set(true);
    while_syncing(&syncing, || assert!(syncing.get()));
    assert!(syncing.get());
}

#[test]
fn test_syncing_guard_restores_previous_state_when_projection_unwinds() {
    let syncing = Cell::new(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while_syncing(&syncing, || panic!("fake projection failure"));
    }));
    assert!(result.is_err());
    assert!(!syncing.get());
}

#[test]
fn test_close_editor_without_ui_cancels_open_reads_but_keeps_confirmation_reads() {
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor());
    tasks.latest_open = 7;
    tasks.queries.insert(7, DetailPurpose::Open(id(1)));
    tasks.queries.insert(8, DetailPurpose::SaveAck(id(1)));
    tasks.queries.insert(9, DetailPurpose::Delete(id(2)));
    tasks.collapsed[5] = Some(true);
    tasks.limits[5] = 600;
    assert_eq!(close_editor(&mut tasks), Some(id(1)));
    assert!(tasks.editor.is_none());
    assert_eq!(tasks.latest_open, 0);
    assert!(!tasks.queries.contains_key(&7));
    assert!(matches!(
        tasks.queries.get(&8),
        Some(DetailPurpose::SaveAck(_))
    ));
    assert!(matches!(
        tasks.queries.get(&9),
        Some(DetailPurpose::Delete(_))
    ));
    assert_eq!(tasks.collapsed[5], Some(true));
    assert_eq!(tasks.limits[5], 600);
}

#[test]
fn test_close_missing_editor_still_cancels_old_open_intent() {
    let mut tasks = TaskSession::new();
    tasks.latest_open = 7;
    tasks.queries.insert(7, DetailPurpose::Open(id(1)));
    assert_eq!(close_editor(&mut tasks), None);
    assert_eq!(tasks.latest_open, 0);
    assert!(tasks.queries.is_empty());
}

#[test]
fn test_reload_without_an_accepted_read_keeps_input_and_kernel_session() {
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor());
    tasks.latest_open = 7;
    tasks.collapsed[5] = Some(false);
    tasks.limits[5] = 600;
    assert!(!discard_for_reload(&mut tasks, id(1)));
    assert_eq!(
        tasks.editor.as_ref().unwrap().fields.title,
        "待保留的假输入"
    );
    assert_eq!(tasks.collapsed[5], Some(false));
    assert_eq!(tasks.limits[5], 600);
    assert!(tasks.queries.is_empty());
}

#[test]
fn test_accepted_reload_discards_old_draft_without_canceling_its_new_open_read() {
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor());
    tasks.latest_open = 7;
    tasks.queries.insert(7, DetailPurpose::Open(id(1)));
    tasks.queries.insert(8, DetailPurpose::SaveAck(id(2)));
    assert!(discard_for_reload(&mut tasks, id(1)));
    assert!(tasks.editor.is_none());
    assert_eq!(tasks.latest_open, 7);
    assert!(matches!(tasks.queries.get(&7), Some(DetailPurpose::Open(target)) if *target == id(1)));
    assert!(tasks.queries.contains_key(&8));
}

#[test]
fn test_reload_does_not_use_an_unrelated_or_wrong_purpose_read_to_discard_input() {
    for purpose in [
        DetailPurpose::Open(id(2)),
        DetailPurpose::Refresh(id(1)),
        DetailPurpose::SaveAck(id(1)),
        DetailPurpose::Delete(id(1)),
    ] {
        let mut tasks = TaskSession::new();
        tasks.editor = Some(editor());
        tasks.latest_open = 7;
        tasks.queries.insert(7, purpose);
        assert!(!discard_for_reload(&mut tasks, id(1)));
        assert_eq!(
            tasks.editor.as_ref().unwrap().fields.title,
            "待保留的假输入"
        );
    }
}

#[test]
fn test_reload_keeps_in_flight_input_even_when_a_new_open_read_exists() {
    let mut tasks = TaskSession::new();
    let mut editor = editor();
    editor.begin_save().unwrap().unwrap();
    editor.committed = true;
    tasks.editor = Some(editor);
    tasks.latest_open = 7;
    tasks.queries.insert(7, DetailPurpose::Open(id(1)));
    assert!(!discard_for_reload(&mut tasks, id(1)));
    let editor = tasks.editor.as_ref().unwrap();
    assert!(editor.in_flight.is_some());
    assert!(editor.committed);
    assert_eq!(editor.fields.title, "待保留的假输入");
}

#[test]
fn test_reload_rejects_a_missing_or_different_selected_editor() {
    let mut tasks = TaskSession::new();
    tasks.latest_open = 7;
    tasks.queries.insert(7, DetailPurpose::Open(id(2)));
    assert!(!discard_for_reload(&mut tasks, id(2)));
    tasks.editor = Some(editor());
    assert!(!discard_for_reload(&mut tasks, id(2)));
    assert_eq!(tasks.editor.as_ref().unwrap().detail.task.id, id(1));
}

#[test]
fn test_editor_focus_rejects_stale_target_hidden_or_loading_editor() {
    assert!(editor_focus_matches(id(1), Some(id(1)), true, false));
    for state in [
        (Some(id(2)), true, false),
        (None, true, false),
        (Some(id(1)), false, false),
        (Some(id(1)), true, true),
    ] {
        assert!(!editor_focus_matches(id(1), state.0, state.1, state.2));
    }
}
