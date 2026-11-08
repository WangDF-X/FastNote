use fastnote_core::{TaskStatus, TodayView};

use crate::tasks::{
    test_support::{detail, id},
    PageScope, PendingTask, Transition, UiEpoch,
};

use super::*;

fn today() -> TodayView {
    TodayView {
        date: "2026-11-04".parse().unwrap(),
        evaluated_at: "2026-11-04T12:00:00".parse().unwrap(),
        next_overdue_at: None,
        sections: Vec::new(),
        open_count: 0,
        overdue_count: 0,
        completed_today_count: 0,
    }
}

#[test]
fn test_release_removes_confirmed_dto_and_long_body_but_keeps_lightweight_session() {
    let mut tasks = TaskSession::new();
    let mut value = detail(1);
    value.task.description = "假".repeat(10_000);
    value.task.validate().unwrap();
    tasks.editor = Some(EditorSession::new(value));
    tasks.today = Some(today());
    tasks.collapsed[3] = Some(true);
    tasks.limits[3] = 1000;
    tasks
        .resume
        .capture(Some(id(1)), "editor:description", -500.0);
    assert!(tasks.release_clean_cache());
    assert!(tasks.today.is_none());
    assert!(tasks.editor.is_none());
    assert_eq!(tasks.resume.selected, Some(id(1)));
    assert_eq!(tasks.resume.focus.as_deref(), Some("editor:description"));
    assert_eq!(tasks.resume.scroll_y, -500.0);
    assert_eq!(tasks.collapsed[3], Some(true));
    assert_eq!(tasks.limits[3], 1000);
}

#[test]
fn test_dirty_in_flight_committed_and_clean_failed_editors_are_never_released() {
    for state in 0..4 {
        let mut tasks = TaskSession::new();
        let mut editor = EditorSession::new(detail(1));
        match state {
            0 => {
                editor.edit("title", "假草稿".to_owned()).unwrap();
            }
            1 => {
                editor.edit("title", "假草稿".to_owned()).unwrap();
                editor.begin_save().unwrap();
            }
            2 => {
                editor.committed = true;
            }
            _ => {
                editor.error = Some("读取失败".to_owned());
            }
        }
        let fields = editor.fields.clone();
        tasks.editor = Some(editor);
        tasks.today = Some(today());
        assert!(tasks.resource_protected());
        assert!(!tasks.release_clean_cache());
        assert_eq!(tasks.editor.as_ref().unwrap().fields, fields);
        assert!(tasks.today.is_some());
    }
}

#[test]
fn test_save_ack_delete_decisions_and_pending_commands_guard_reclamation() {
    for state in 0..8 {
        let mut tasks = TaskSession::new();
        match state {
            0 => {
                tasks.pending.insert(1, PendingTask::Priority);
            }
            1 => {
                tasks.queries.insert(1, DetailPurpose::SaveAck(id(1)));
            }
            2 => {
                tasks.queries.insert(1, DetailPurpose::Delete(id(1)));
            }
            3 => {
                tasks.after_save = Some(Transition::Close { exit: false });
            }
            4 => {
                tasks.delete_confirmation = Some(id(1));
            }
            5 => {
                tasks.status_confirmation = Some((id(1), TaskStatus::Done));
            }
            6 => {
                tasks.reload_confirmation = Some(id(1));
            }
            _ => {
                tasks.discard_exit_allowed = true;
            }
        }
        assert!(tasks.resource_protected());
        assert!(!tasks.release_clean_cache());
    }
}

#[test]
fn test_cancelled_page_reads_do_not_cancel_required_kernel_confirmation_reads() {
    let mut tasks = TaskSession::new();
    let mut epoch = UiEpoch::default();
    assert!(epoch.mount());
    let stamp = epoch.stamp(PageScope::Today).unwrap();
    tasks.latest_today = 1;
    tasks.latest_open = 2;
    tasks.queries.insert(2, DetailPurpose::Open(id(1)));
    tasks.queries.insert(3, DetailPurpose::Refresh(id(1)));
    tasks.queries.insert(4, DetailPurpose::SaveAck(id(1)));
    tasks.queries.insert(5, DetailPurpose::Delete(id(1)));
    tasks
        .read_stamps
        .extend([(1, stamp), (2, stamp), (3, stamp)]);
    tasks.cancel_page_reads();
    assert_eq!(tasks.latest_today, 0);
    assert_eq!(tasks.latest_open, 0);
    assert!(tasks.read_stamps.is_empty());
    assert_eq!(tasks.queries.len(), 2);
    assert!(matches!(
        tasks.queries.get(&4),
        Some(DetailPurpose::SaveAck(_))
    ));
    assert!(matches!(
        tasks.queries.get(&5),
        Some(DetailPurpose::Delete(_))
    ));
}

#[test]
fn test_restore_focus_is_a_bounded_whitelist_and_scroll_rejects_non_finite_offsets() {
    assert_eq!(
        focus_anchor("editor:title").as_deref(),
        Some("editor:title")
    );
    assert!(focus_anchor(&format!("task:{}", id(1))).is_some());
    assert!(focus_anchor("today:section:7").is_some());
    for anchor in [
        "task:假正文",
        "today:section:8",
        "today:more:999",
        "任意正文",
        "editor:arbitrary",
    ] {
        assert_eq!(focus_anchor(anchor), None);
    }
    assert_eq!(
        focus_anchor(&format!("today:section:{}1", "0".repeat(1000))),
        None
    );
    let mut resume = TaskResume::default();
    for offset in [f32::NAN, f32::INFINITY, 500.0] {
        resume.capture(None, "", offset);
        assert_eq!(resume.scroll_y, 0.0);
    }
}

#[test]
fn test_cancelled_decision_releases_only_matching_permission_and_not_a_dirty_draft() {
    let mut tasks = TaskSession::new();
    tasks.delete_confirmation = Some(id(1));
    tasks.status_confirmation = Some((id(2), TaskStatus::Done));
    tasks.cancel_confirmation(&format!("task:delete-confirm:{}", id(2)));
    assert_eq!(tasks.delete_confirmation, Some(id(1)));
    tasks.cancel_confirmation(&format!("task:delete-confirm:{}", id(1)));
    assert_eq!(tasks.delete_confirmation, None);
    assert!(tasks.status_confirmation.is_some());
    tasks.cancel_confirmation(&format!("task:status-confirm:{}", id(2)));
    assert!(!tasks.resource_protected());
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "假草稿".to_owned()).unwrap();
    tasks.editor = Some(editor);
    tasks.discard_exit_allowed = true;
    tasks.cancel_confirmation("task:discard-and-quit");
    assert!(!tasks.discard_exit_allowed);
    assert!(tasks.resource_protected());
}

#[test]
fn test_clean_failed_editor_still_blocks_normal_transition_until_successful_refresh() {
    let mut tasks = TaskSession::new();
    let mut editor = EditorSession::new(detail(1));
    editor.error = Some("刷新失败".to_owned());
    tasks.editor = Some(editor);
    tasks.after_save = Some(Transition::Close { exit: false });
    assert!(tasks.take_ready_transition().is_none());
    tasks.editor.as_mut().unwrap().refresh(detail(1));
    assert!(matches!(
        tasks.take_ready_transition(),
        Some(Transition::Close { exit: false })
    ));
}
