use std::cell::{Cell, RefCell};

use fastnote_core::{Task, TaskContext, TaskDetail, TaskId, TaskStatus};

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
    EditorSession::new(TaskDetail {
        task,
        context: TaskContext::default(),
        is_overdue: false,
        is_blocked: false,
    })
}

#[test]
fn test_kernel_requires_ready_and_rejects_finished_closing_or_busy() {
    assert!(kernel_available(true, false, false, false));
    for state in [
        (false, false, false, false),
        (true, true, false, false),
        (true, false, true, false),
        (true, false, false, true),
    ] {
        assert!(!kernel_available(state.0, state.1, state.2, state.3));
    }
}

#[test]
fn test_no_ui_page_queries_are_rejected_but_confirmation_reads_remain_allowed() {
    assert!(!may_query(None, false));
    for purpose in [DetailPurpose::Open(id(1)), DetailPurpose::Refresh(id(1))] {
        assert!(!may_query(Some(purpose), false));
    }
    for purpose in [DetailPurpose::SaveAck(id(1)), DetailPurpose::Delete(id(1))] {
        assert!(may_query(Some(purpose), false));
    }
}

#[test]
fn test_visible_today_page_permits_page_and_confirmation_reads() {
    assert!(may_query(None, true));
    for purpose in [
        DetailPurpose::Open(id(1)),
        DetailPurpose::Refresh(id(1)),
        DetailPurpose::SaveAck(id(1)),
        DetailPurpose::Delete(id(1)),
    ] {
        assert!(may_query(Some(purpose), true));
    }
}

#[test]
fn test_page_visibility_requires_native_content_and_both_confirmed_and_projected_today_route() {
    assert!(today_page_visible(true, "今日任务", "今日任务"));
    for state in [
        (false, "今日任务", "今日任务"),
        (true, "日历", "日历"),
        (true, "今日任务", "日历"),
        (true, "日历", "今日任务"),
    ] {
        assert!(!today_page_visible(state.0, state.1, state.2));
        assert!(!may_query(
            None,
            today_page_visible(state.0, state.1, state.2)
        ));
    }
}

#[test]
fn test_page_load_only_queries_missing_or_invalid_snapshot_without_an_in_flight_read() {
    assert!(needs_page_load(true, true, 0));
    assert!(needs_page_load(false, false, 0));
    assert!(!needs_page_load(false, true, 0));
    for (dirty, snapshot) in [(false, false), (false, true), (true, false), (true, true)] {
        assert!(!needs_page_load(dirty, snapshot, 7));
    }
}

#[test]
fn test_detail_query_deduplication_reuses_only_same_task_and_same_purpose() {
    let mut tasks = TaskSession::new();
    assert!(!detail_query_pending(&tasks, DetailPurpose::SaveAck(id(1))));
    tasks.queries.insert(7, DetailPurpose::SaveAck(id(1)));
    assert!(detail_query_pending(&tasks, DetailPurpose::SaveAck(id(1))));
    assert!(!detail_query_pending(&tasks, DetailPurpose::SaveAck(id(2))));
    assert!(!detail_query_pending(&tasks, DetailPurpose::Refresh(id(1))));
    tasks.queries.insert(8, DetailPurpose::Refresh(id(2)));
    assert!(detail_query_pending(&tasks, DetailPurpose::Refresh(id(2))));
    assert!(!detail_query_pending(&tasks, DetailPurpose::Refresh(id(1))));
    assert!(!detail_query_pending(&tasks, DetailPurpose::SaveAck(id(2))));
}

#[test]
fn test_open_and_delete_requests_are_not_silently_deduplicated() {
    let mut tasks = TaskSession::new();
    tasks.queries.insert(7, DetailPurpose::Open(id(1)));
    tasks.queries.insert(8, DetailPurpose::Delete(id(1)));
    assert!(!detail_query_pending(&tasks, DetailPurpose::Open(id(1))));
    assert!(!detail_query_pending(&tasks, DetailPurpose::Delete(id(1))));
}

#[test]
fn test_save_read_failure_preserves_committed_input_and_in_flight_generation() {
    let mut editor = editor();
    editor.edit("title", "需要保留的假输入".to_owned()).unwrap();
    let generation = editor.begin_save().unwrap().unwrap().generation;
    editor.committed = true;
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    save_read_failed(&mut tasks, DetailPurpose::SaveAck(id(1)), "查询队列不可用");
    let editor = tasks.editor.as_ref().unwrap();
    assert_eq!(editor.fields.title, "需要保留的假输入");
    assert!(editor.committed);
    assert_eq!(editor.in_flight.as_ref().unwrap().generation, generation);
    assert_eq!(editor.error.as_deref(), Some("查询队列不可用"));
    assert!(tasks.unfinished());
}

#[test]
fn test_other_task_or_page_read_failure_does_not_change_selected_editor() {
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor());
    for purpose in [
        DetailPurpose::SaveAck(id(2)),
        DetailPurpose::Open(id(1)),
        DetailPurpose::Refresh(id(1)),
        DetailPurpose::Delete(id(1)),
    ] {
        save_read_failed(&mut tasks, purpose, "旧查询失败");
        assert!(tasks.editor.as_ref().unwrap().error.is_none());
    }
}

#[test]
fn test_save_read_failure_without_editor_is_safe() {
    let mut tasks = TaskSession::new();
    save_read_failed(&mut tasks, DetailPurpose::SaveAck(id(1)), "查询失败");
    assert!(tasks.editor.is_none());
}

#[test]
fn test_focus_requires_visible_native_today_page_and_no_dialog() {
    for state in [
        (false, true, Some(false), "今日任务", false),
        (true, false, Some(false), "今日任务", false),
        (true, true, Some(true), "今日任务", false),
        (true, true, Some(false), "设置", false),
        (true, true, Some(false), "日历", false),
        (true, true, Some(false), "今日任务", true),
    ] {
        assert!(!may_focus_tasks(
            state.0, state.1, state.2, state.3, state.4
        ));
    }
}

#[test]
fn test_focus_accepts_unminimized_or_unknown_minimize_state_with_native_window() {
    assert!(may_focus_tasks(true, true, Some(false), "今日任务", false));
    assert!(may_focus_tasks(true, true, None, "今日任务", false));
    assert!(!may_focus_tasks(true, false, None, "今日任务", false));
}

#[test]
fn test_wrong_or_malformed_confirmation_keeps_original_decision() {
    let tasks = RefCell::new(TaskSession::new());
    tasks.borrow_mut().delete_confirmation = Some(id(1));
    let executed = Cell::new(0);
    for target in [Some(id(2)), None] {
        assert!(!resolve_confirmation(
            || tasks
                .borrow_mut()
                .delete_confirmation
                .take_if(|value| Some(*value) == target),
            |_| {
                executed.set(executed.get() + 1);
                true
            },
            |previous| tasks.borrow_mut().delete_confirmation = Some(previous),
        ));
        assert_eq!(tasks.borrow().delete_confirmation, Some(id(1)));
    }
    assert_eq!(executed.get(), 0);
}

#[test]
fn test_matching_confirmation_is_consumed_once_without_holding_kernel_borrow() {
    let tasks = RefCell::new(TaskSession::new());
    tasks.borrow_mut().status_confirmation = Some((id(1), TaskStatus::Done));
    let executed = Cell::new(0);
    for expected in [true, false] {
        assert_eq!(
            resolve_confirmation(
                || tasks
                    .borrow_mut()
                    .status_confirmation
                    .take_if(|(target, _)| *target == id(1)),
                |value| {
                    assert_eq!(*value, (id(1), TaskStatus::Done));
                    assert!(tasks.borrow().status_confirmation.is_none());
                    tasks.borrow_mut().today = None;
                    executed.set(executed.get() + 1);
                    true
                },
                |previous| tasks.borrow_mut().status_confirmation = Some(previous),
            ),
            expected
        );
    }
    assert_eq!(executed.get(), 1);
}

#[test]
fn test_queue_rejection_restores_confirmation_without_losing_input() {
    let tasks = RefCell::new(TaskSession::new());
    let mut editor = editor();
    editor
        .edit("description", "尚未保存的假输入".to_owned())
        .unwrap();
    tasks.borrow_mut().editor = Some(editor);
    tasks.borrow_mut().delete_confirmation = Some(id(1));
    assert!(!resolve_confirmation(
        || tasks
            .borrow_mut()
            .delete_confirmation
            .take_if(|target| *target == id(1)),
        |_| {
            assert!(tasks.borrow().delete_confirmation.is_none());
            assert_eq!(
                tasks.borrow().editor.as_ref().unwrap().fields.description,
                "尚未保存的假输入"
            );
            false
        },
        |previous| tasks.borrow_mut().delete_confirmation = Some(previous),
    ));
    assert_eq!(tasks.borrow().delete_confirmation, Some(id(1)));
    assert!(tasks.borrow().editor.as_ref().unwrap().dirty());
}

#[test]
fn test_confirmed_action_without_ui_uses_kernel_and_never_creates_resources() {
    let ui = RefCell::new(None::<u8>);
    let tasks = RefCell::new(TaskSession::new());
    tasks.borrow_mut().delete_confirmation = Some(id(1));
    let executed = Cell::new(0);
    assert!(kernel_available(true, false, false, false));
    assert!(resolve_confirmation(
        || tasks
            .borrow_mut()
            .delete_confirmation
            .take_if(|target| *target == id(1)),
        |_| {
            assert!(with_session(&ui, |_| unreachable!()).is_none());
            executed.set(executed.get() + 1);
            true
        },
        |previous| tasks.borrow_mut().delete_confirmation = Some(previous),
    ));
    assert_eq!(*ui.borrow(), None);
    assert_eq!(executed.get(), 1);
}

#[test]
fn test_unavailable_kernel_does_not_consume_confirmation() {
    let permission = RefCell::new(Some(id(1)));
    let executed = Cell::new(false);
    assert!(!resolve_confirmation(
        || {
            if kernel_available(true, false, false, true) {
                permission.borrow_mut().take()
            } else {
                None
            }
        },
        |_| {
            executed.set(true);
            true
        },
        |previous| *permission.borrow_mut() = Some(previous),
    ));
    assert_eq!(*permission.borrow(), Some(id(1)));
    assert!(!executed.get());
}

#[test]
fn test_rejected_reload_dialog_restores_previous_permission_and_keeps_draft() {
    let tasks = RefCell::new(TaskSession::new());
    let mut editor = editor();
    editor.edit("title", "不能丢弃的假输入".to_owned()).unwrap();
    tasks.borrow_mut().editor = Some(editor);
    tasks.borrow_mut().reload_confirmation = Some(id(2));
    assert!(!stage_confirmation(
        || tasks.borrow_mut().reload_confirmation.replace(id(1)),
        || {
            assert_eq!(tasks.borrow().reload_confirmation, Some(id(1)));
            false
        },
        |previous| tasks.borrow_mut().reload_confirmation = previous,
    ));
    assert_eq!(tasks.borrow().reload_confirmation, Some(id(2)));
    assert_eq!(
        tasks.borrow().editor.as_ref().unwrap().fields.title,
        "不能丢弃的假输入"
    );
}
