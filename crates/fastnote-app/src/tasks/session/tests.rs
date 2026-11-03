use fastnote_core::{TaskSummary, TodaySection, TodaySectionKind};

use crate::tasks::test_support::{detail, id};

use super::*;

fn session() -> TaskSession {
    let mut session = TaskSession::new();
    session.today = Some(TodayView {
        date: "2026-11-03".parse().expect("fixed date"),
        evaluated_at: "2026-11-03T12:00:00".parse().unwrap(),
        next_overdue_at: None,
        open_count: 2,
        overdue_count: 0,
        completed_today_count: 1,
        tomorrow_count: 0,
        sections: vec![
            TodaySection {
                kind: TodaySectionKind::Unscheduled,
                items: vec![
                    TaskSummary::from_task(&detail(1).task, Default::default()),
                    TaskSummary::from_task(&detail(2).task, Default::default()),
                ],
                collapsed_default: false,
                total: 2,
                has_more: false,
            },
            TodaySection {
                kind: TodaySectionKind::CompletedToday,
                items: vec![TaskSummary::from_task(&detail(3).task, Default::default())],
                collapsed_default: true,
                total: 1,
                has_more: false,
            },
        ],
    });
    session
}

#[test]
fn test_collapsing_groups_preserves_other_groups_and_controls_focus_order() {
    let mut session = session();
    assert_eq!(session.visible_ids(), vec![id(1), id(2)]);
    assert!(session.toggle_section(7));
    assert_eq!(session.visible_ids(), vec![id(1), id(2), id(3)]);
    assert!(session.toggle_section(5));
    assert_eq!(session.visible_ids(), vec![id(3)]);
    assert!(!session.toggle_section(100));
    assert!(!session.toggle_section(0));
}

#[test]
fn test_load_limits_are_per_group_capped_and_do_not_persist() {
    let mut session = session();
    for _ in 0..4 {
        assert!(session.load_more(5));
    }
    assert_eq!(session.limits[5], 1000);
    assert_eq!(session.limits[7], 200);
    assert!(!session.load_more(5) && !session.load_more(99));
    assert_eq!(TaskSession::new().limits, [200; 8]);
}

#[test]
fn test_focus_neighbors_are_bounded_and_ignore_hidden_rows() {
    let session = session();
    assert_eq!(session.focus_neighbor(id(1), -1), Some(id(1)));
    assert_eq!(session.focus_neighbor(id(1), 1), Some(id(2)));
    assert_eq!(session.focus_neighbor(id(2), 1), Some(id(2)));
    assert_eq!(session.focus_neighbor(id(3), -1), None);
}

#[test]
fn test_pending_writes_and_dirty_or_unacknowledged_editor_block_transitions() {
    let mut session = session();
    assert!(!session.unfinished());
    session.pending.insert(1, PendingTask::Priority);
    assert!(session.unfinished() && session.non_text_pending());
    session.pending.clear();
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "假草稿".to_owned()).unwrap();
    session.editor = Some(editor);
    assert!(session.unfinished());
    session.pending.insert(2, PendingTask::Save { id: id(1) });
    assert!(!session.non_text_pending());
}

#[test]
fn test_closing_detail_cancels_pending_open_but_keeps_save_ack_queries() {
    let mut session = session();
    session.latest_open = 1;
    session.queries.insert(1, DetailPurpose::Open(id(1)));
    session.queries.insert(2, DetailPurpose::SaveAck(id(1)));
    session.cancel_open_requests();
    assert_eq!(session.latest_open, 0);
    assert!(!session.queries.contains_key(&1));
    assert!(session.queries.contains_key(&2));
}

#[test]
fn test_metadata_reload_blocks_non_text_mutations_without_discarding_draft() {
    let mut session = session();
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "仍需保留的假草稿".to_owned()).unwrap();
    session.editor = Some(editor);
    session.queries.insert(1, DetailPurpose::Refresh(id(1)));
    assert!(session.non_text_pending());
    session.queries.remove(&1);
    assert!(!session.non_text_pending());
    assert_eq!(
        session.editor.as_ref().unwrap().fields.title,
        "仍需保留的假草稿"
    );
}

#[test]
fn test_date_entry_and_title_entry_restore_the_correct_editor_focus() {
    assert_eq!(Transition::Open(id(1)).editor_focus(), Some("editor:title"));
    assert_eq!(
        Transition::OpenDates(id(1)).editor_focus(),
        Some("editor:start")
    );
    assert_eq!(Transition::CloseDetail.editor_focus(), None);
    // 等待保存的动作也保留「设置日期」意图，不会被默认标题焦点覆盖。
    let mut session = session();
    session.after_save = Some(Transition::OpenDates(id(1)));
    assert_eq!(
        session
            .after_save
            .as_ref()
            .and_then(Transition::editor_focus),
        Some("editor:start")
    );
}

#[test]
fn test_opening_dates_for_current_task_preserves_an_invalid_draft() {
    let mut session = session();
    let mut editor = EditorSession::new(detail(1));
    editor.edit("start_date", "2026-11-04".to_owned()).unwrap();
    editor.edit("due_date", "2026-11-03".to_owned()).unwrap();
    session.editor = Some(editor);
    assert_eq!(
        session.current_editor_focus(&Transition::OpenDates(id(1))),
        Some("editor:start")
    );
    assert_eq!(
        session.current_editor_focus(&Transition::Open(id(1))),
        Some("editor:title")
    );
    assert_eq!(
        session.current_editor_focus(&Transition::OpenDates(id(2))),
        None
    );
    assert!(session.editor.as_ref().unwrap().dirty());
    assert_eq!(
        session.editor.as_ref().unwrap().fields.start_date,
        "2026-11-04"
    );
}

#[test]
fn test_clock_refresh_handles_due_boundary_midnight_and_clock_rollback() {
    let mut session = session();
    session.today.as_mut().unwrap().next_overdue_at = Some("2026-11-03T15:30:00".parse().unwrap());
    assert!(!session.needs_clock_refresh("2026-11-03T15:30:00".parse().unwrap()));
    assert!(session.needs_clock_refresh("2026-11-03T15:30:01".parse().unwrap()));
    assert!(session.needs_clock_refresh("2026-11-04T00:00:00".parse().unwrap()));
    assert!(session.needs_clock_refresh("2026-11-03T11:00:00".parse().unwrap()));
    assert!(!session.needs_clock_refresh("2026-11-03T13:00:00".parse().unwrap()));
}

#[test]
fn test_outside_close_waits_for_save_ack_and_does_not_discard_failed_draft() {
    let mut session = session();
    let mut editor = EditorSession::new(detail(1));
    editor
        .edit("title", "外部点击前的假输入".to_owned())
        .unwrap();
    session.editor = Some(editor);
    session.after_save = Some(Transition::CloseDetail);
    assert!(session.take_ready_transition().is_none());
    let snapshot = session
        .editor
        .as_mut()
        .unwrap()
        .begin_save()
        .unwrap()
        .unwrap();
    session.editor.as_mut().unwrap().committed = true;
    assert!(session.take_ready_transition().is_none());
    let mut ack = detail(1);
    ack.task = ack
        .task
        .patched(&snapshot.patch, ack.task.updated_at)
        .unwrap();
    session.editor.as_mut().unwrap().acknowledge(ack);
    assert!(matches!(
        session.take_ready_transition(),
        Some(Transition::CloseDetail)
    ));
    session
        .editor
        .as_mut()
        .unwrap()
        .edit("title", "失败后不能丢弃".to_owned())
        .unwrap();
    session
        .editor
        .as_mut()
        .unwrap()
        .failed("模拟失败".to_owned());
    session.after_save = Some(Transition::CloseDetail);
    assert!(session.take_ready_transition().is_none());
    assert_eq!(
        session.editor.as_ref().unwrap().fields.title,
        "失败后不能丢弃"
    );
}

#[test]
fn test_explicit_unscheduled_reveal_only_expands_that_group_and_preserves_pagination() {
    let mut session = session();
    session.collapsed = [Some(true); 8];
    session.limits = [400; 8];
    session.request_unscheduled_reveal();
    assert_eq!(session.collapsed[5], Some(false));
    for index in [0, 1, 2, 3, 4, 6, 7] {
        assert_eq!(session.collapsed[index], Some(true));
    }
    assert_eq!(session.limits, [400; 8]);
    assert!(session.take_unscheduled_reveal());
    assert!(!session.take_unscheduled_reveal());
}

#[test]
fn test_absent_or_empty_reveal_target_is_consumed_without_a_future_scroll_request() {
    let mut session = session();
    session
        .today
        .as_mut()
        .unwrap()
        .sections
        .retain(|section| section.kind != TodaySectionKind::Unscheduled);
    session.request_unscheduled_reveal();
    assert!(!session.take_unscheduled_reveal());
    session.today = self::session().today;
    assert!(!session.take_unscheduled_reveal());
    session.today = None;
    session.request_unscheduled_reveal();
    assert!(!session.take_unscheduled_reveal());
}
