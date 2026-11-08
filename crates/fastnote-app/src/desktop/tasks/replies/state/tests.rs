use std::cell::RefCell;

use fastnote_core::{
    DateTime, NaiveDate, NaiveDateTime, Task, TaskContext, TaskError, TaskStatus, Utc,
};

use crate::tasks::{EditorSession, Transition};

use super::*;

const FIRST: &str = "019a0fff-aaaa-7000-8000-000000000001";
const SECOND: &str = "019a0fff-aaaa-7000-8000-000000000002";

fn fake_detail(id: TaskId, now: DateTime<Utc>) -> Result<TaskDetail, TaskError> {
    Task::new(id, "假任务".to_owned(), now).map(|task| TaskDetail {
        task,
        context: TaskContext::default(),
        is_overdue: false,
        is_blocked: false,
    })
}

fn fake_today(date: NaiveDate, evaluated_at: NaiveDateTime) -> TodayView {
    TodayView {
        date,
        evaluated_at,
        next_overdue_at: None,
        sections: Vec::new(),
        open_count: 0,
        overdue_count: 0,
        completed_today_count: 0,
    }
}

#[test]
fn test_no_ui_today_reply_does_not_cache_snapshot_or_change_fold_and_page_state() {
    let date = "2026-11-04".parse().unwrap();
    let at = "2026-11-04T12:00:00".parse().unwrap();
    let mut tasks = TaskSession::new();
    tasks.latest_today = 7;
    tasks.collapsed[1] = Some(true);
    tasks.limits[1] = 600;
    assert!(!adopt_today(&mut tasks, 7, fake_today(date, at), false));
    assert!(tasks.today.is_none());
    assert_eq!(tasks.collapsed[1], Some(true));
    assert_eq!(tasks.limits[1], 600);
}

#[test]
fn test_current_today_reply_is_cached_only_with_a_visible_page() {
    let date = "2026-11-04".parse().unwrap();
    let at = "2026-11-04T12:00:00".parse().unwrap();
    let mut tasks = TaskSession::new();
    tasks.latest_today = 7;
    let view = fake_today(date, at);
    assert!(adopt_today(&mut tasks, 7, view.clone(), true));
    assert_eq!(tasks.today, Some(view));
}

#[test]
fn test_stale_and_zero_today_reply_do_not_replace_confirmed_snapshot() {
    let date = "2026-11-04".parse().unwrap();
    let at = "2026-11-04T12:00:00".parse().unwrap();
    let previous = fake_today(date, at);
    let mut tasks = TaskSession::new();
    tasks.today = Some(previous.clone());
    tasks.latest_today = 8;
    assert!(!adopt_today(&mut tasks, 7, previous.clone(), true));
    tasks.latest_today = 0;
    assert!(!adopt_today(&mut tasks, 0, previous.clone(), true));
    assert_eq!(tasks.today, Some(previous));
}

#[test]
fn test_no_ui_detail_policy_keeps_save_and_delete_confirmation_but_not_page_reads() {
    let id = FIRST.parse().unwrap();
    for purpose in [DetailPurpose::Open(id), DetailPurpose::Refresh(id)] {
        assert!(!may_apply_detail(purpose, false));
        assert!(may_apply_detail(purpose, true));
    }
    for purpose in [DetailPurpose::SaveAck(id), DetailPurpose::Delete(id)] {
        assert!(may_apply_detail(purpose, false));
        assert!(may_apply_detail(purpose, true));
    }
}

#[test]
fn test_all_and_today_events_mark_only_relevant_implemented_scopes() {
    let id = FIRST.parse().unwrap();
    let all = [AppEvent::TasksChanged {
        scope: TaskScope::All,
        affected: vec![id],
    }];
    assert_eq!(
        invalidations(&all, Some(id)),
        Invalidations {
            today: true,
            detail: true
        }
    );
    assert_eq!(
        invalidations(&all, None),
        Invalidations {
            today: true,
            detail: false
        }
    );
    let today = [AppEvent::TasksChanged {
        scope: TaskScope::Today,
        affected: vec![id],
    }];
    assert_eq!(
        invalidations(&today, Some(id)),
        Invalidations {
            today: true,
            detail: false
        }
    );
    assert!(!should_refresh(true, false));
    assert!(!should_refresh(false, true));
    assert!(should_refresh(true, true));
}

#[test]
fn test_detail_and_unimplemented_scope_events_do_not_preload_other_pages() {
    let id = FIRST.parse().unwrap();
    let other = SECOND.parse().unwrap();
    let detail = [AppEvent::TasksChanged {
        scope: TaskScope::Detail(id),
        affected: vec![id],
    }];
    assert_eq!(
        invalidations(&detail, Some(id)),
        Invalidations {
            today: false,
            detail: true
        }
    );
    assert_eq!(
        invalidations(&detail, Some(other)),
        Invalidations::default()
    );
    assert_eq!(invalidations(&detail, None), Invalidations::default());
    let unrelated = [
        AppEvent::TasksChanged {
            scope: TaskScope::Calendar,
            affected: vec![id],
        },
        AppEvent::NotesChanged,
    ];
    assert_eq!(
        invalidations(&unrelated, Some(id)),
        Invalidations::default()
    );
    assert_eq!(invalidations(&[], Some(id)), Invalidations::default());
}

#[test]
fn test_commit_ack_without_ui_keeps_protected_in_flight_save_until_confirmation_read() {
    let id = FIRST.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut editor = EditorSession::new(fake_detail(id, now).unwrap());
    editor.edit("title", "假草稿".to_owned()).unwrap();
    editor.begin_save().unwrap().unwrap();
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    assert!(mark_save_committed(&mut tasks, id));
    let editor = tasks.editor.as_ref().unwrap();
    assert!(editor.committed);
    assert!(editor.in_flight.is_some());
    assert_eq!(editor.fields.title, "假草稿");
    assert!(tasks.unfinished());
}

#[test]
fn test_unrelated_or_missing_save_ack_does_not_mark_an_editor_committed() {
    let id = FIRST.parse().unwrap();
    let other = SECOND.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut tasks = TaskSession::new();
    assert!(!mark_save_committed(&mut tasks, id));
    tasks.editor = Some(EditorSession::new(fake_detail(id, now).unwrap()));
    assert!(!mark_save_committed(&mut tasks, other));
    assert!(!mark_save_committed(&mut tasks, id));
    assert!(!tasks.editor.as_ref().unwrap().committed);
}

#[test]
fn test_failed_save_without_ui_preserves_input_and_clears_only_its_in_flight_state() {
    let id = FIRST.parse().unwrap();
    let other = SECOND.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut editor = EditorSession::new(fake_detail(id, now).unwrap());
    editor.edit("title", "需要保留的假输入".to_owned()).unwrap();
    editor.begin_save().unwrap().unwrap();
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    assert!(!mark_save_failed(
        &mut tasks,
        other,
        "旧任务失败".to_owned()
    ));
    assert!(tasks.editor.as_ref().unwrap().in_flight.is_some());
    assert!(mark_save_failed(&mut tasks, id, "保存失败".to_owned()));
    let editor = tasks.editor.as_ref().unwrap();
    assert_eq!(editor.fields.title, "需要保留的假输入");
    assert_eq!(editor.error.as_deref(), Some("保存失败"));
    assert!(editor.in_flight.is_none());
    assert!(!editor.committed);
    assert!(tasks.unfinished());
}

#[test]
fn test_save_confirmation_without_ui_preserves_newer_typing_and_does_not_advance_transition() {
    let id = FIRST.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut editor = EditorSession::new(fake_detail(id, now).unwrap());
    editor.edit("title", "第一个假版本".to_owned()).unwrap();
    editor.begin_save().unwrap().unwrap();
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    tasks.after_save = Some(Transition::Navigate("navigate:日历".to_owned()));
    assert!(mark_save_committed(&mut tasks, id));
    tasks
        .editor
        .as_mut()
        .unwrap()
        .edit("title", "更新的假版本".to_owned())
        .unwrap();
    let mut saved = fake_detail(id, now).unwrap();
    saved.task.title = "第一个假版本".to_owned();
    assert!(acknowledge_save(&mut tasks, id, saved));
    let editor = tasks.editor.as_ref().unwrap();
    assert_eq!(editor.fields.title, "更新的假版本");
    assert!(editor.in_flight.is_none());
    assert!(!editor.committed);
    assert!(tasks.take_ready_transition().is_none());
}

#[test]
fn test_matching_save_confirmation_releases_transition_once_without_resubmitting_save() {
    let id = FIRST.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut editor = EditorSession::new(fake_detail(id, now).unwrap());
    editor.edit("title", "已提交的假输入".to_owned()).unwrap();
    editor.begin_save().unwrap().unwrap();
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    tasks.after_save = Some(Transition::Close { exit: false });
    assert!(mark_save_committed(&mut tasks, id));
    let mut saved = fake_detail(id, now).unwrap();
    saved.task.title = "已提交的假输入".to_owned();
    assert!(acknowledge_save(&mut tasks, id, saved.clone()));
    assert!(!acknowledge_save(&mut tasks, id, saved));
    assert!(matches!(
        tasks.take_ready_transition(),
        Some(Transition::Close { exit: false })
    ));
    assert!(tasks.take_ready_transition().is_none());
}

#[test]
fn test_wrong_save_confirmation_does_not_clear_protected_input() {
    let id = FIRST.parse().unwrap();
    let other = SECOND.parse().unwrap();
    let now = "2026-11-04T12:00:00Z".parse().unwrap();
    let mut editor = EditorSession::new(fake_detail(id, now).unwrap());
    editor.edit("title", "仍在保存的假输入".to_owned()).unwrap();
    editor.begin_save().unwrap().unwrap();
    let mut tasks = TaskSession::new();
    tasks.editor = Some(editor);
    assert!(mark_save_committed(&mut tasks, id));
    assert!(!acknowledge_save(
        &mut tasks,
        id,
        fake_detail(other, now).unwrap()
    ));
    assert!(!acknowledge_save(
        &mut tasks,
        other,
        fake_detail(other, now).unwrap()
    ));
    assert!(tasks.editor.as_ref().unwrap().in_flight.is_some());
    assert!(tasks.editor.as_ref().unwrap().committed);
}

#[test]
fn test_confirmation_is_staged_before_projection_without_holding_kernel_borrow() {
    let id = FIRST.parse().unwrap();
    let tasks = RefCell::new(TaskSession::new());
    assert!(stage_confirmation(
        || tasks
            .borrow_mut()
            .status_confirmation
            .replace((id, TaskStatus::Done)),
        || {
            assert_eq!(
                tasks.borrow().status_confirmation,
                Some((id, TaskStatus::Done))
            );
            tasks.borrow_mut().after_save = None;
            true
        },
        |previous| tasks.borrow_mut().status_confirmation = previous,
    ));
    assert_eq!(
        tasks.borrow().status_confirmation,
        Some((id, TaskStatus::Done))
    );
}

#[test]
fn test_rejected_confirmation_restores_previous_decision_without_executing_it() {
    let id = FIRST.parse().unwrap();
    let other = SECOND.parse().unwrap();
    let tasks = RefCell::new(TaskSession::new());
    tasks.borrow_mut().delete_confirmation = Some(other);
    assert!(!stage_confirmation(
        || tasks.borrow_mut().delete_confirmation.replace(id),
        || {
            assert_eq!(tasks.borrow().delete_confirmation, Some(id));
            false
        },
        |previous| tasks.borrow_mut().delete_confirmation = previous,
    ));
    assert_eq!(tasks.borrow().delete_confirmation, Some(other));
}
