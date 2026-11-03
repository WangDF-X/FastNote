use crate::tasks::test_support::detail;

use super::*;

#[test]
fn test_old_save_reply_never_overwrites_newer_typing_and_next_save_uses_new_baseline() {
    let original = detail(1);
    let mut editor = EditorSession::new(original.clone());
    editor.edit("title", "第一份假输入".to_owned()).unwrap();
    let first = editor.begin_save().unwrap().unwrap();
    editor.edit("title", "第二份假输入".to_owned()).unwrap();
    assert!(editor.begin_save().unwrap().is_none());
    editor.committed = true;
    let mut ack = original;
    ack.task = ack
        .task
        .patched(
            &first.patch,
            ack.task.updated_at + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert!(editor.acknowledge(ack.clone()));
    assert_eq!(editor.fields.title, "第二份假输入");
    assert!(editor.dirty());
    let second = editor.begin_save().unwrap().unwrap();
    assert_eq!(second.expected_updated_at, ack.task.updated_at);
    assert_eq!(second.patch.title.as_deref(), Some("第二份假输入"));
    editor.committed = true;
    ack.task = ack
        .task
        .patched(
            &second.patch,
            ack.task.updated_at + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert!(editor.acknowledge(ack));
    assert!(!editor.unfinished());
}

#[test]
fn test_same_generation_ack_normalizes_title_without_resetting_description() {
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "  假标题  ".to_owned()).unwrap();
    editor
        .edit("description", "\n正文原样\n".to_owned())
        .unwrap();
    let save = editor.begin_save().unwrap().unwrap();
    let mut ack = detail(1);
    ack.task = ack.task.patched(&save.patch, ack.task.updated_at).unwrap();
    editor.committed = true;
    assert!(editor.acknowledge(ack));
    assert_eq!(editor.fields.title, "假标题");
    assert_eq!(editor.fields.description, "\n正文原样\n");
    assert!(!editor.dirty());
}

#[test]
fn test_failed_save_keeps_input_and_allows_retry_without_a_false_ack() {
    let mut editor = EditorSession::new(detail(1));
    editor
        .edit("description", "不能丢失的假输入".to_owned())
        .unwrap();
    assert!(editor.begin_save().unwrap().is_some());
    assert!(!editor.acknowledge(detail(2)));
    editor.failed("模拟存储错误".to_owned());
    assert!(editor.unfinished());
    assert_eq!(editor.fields.description, "不能丢失的假输入");
    assert!(editor.begin_save().unwrap().is_some());
}

#[test]
fn test_invalid_date_draft_is_preserved_and_does_not_enter_in_flight_state() {
    let mut editor = EditorSession::new(detail(1));
    editor.edit("start_date", "2026-11-04".to_owned()).unwrap();
    editor.edit("due_date", "2026-11-03".to_owned()).unwrap();
    assert!(matches!(editor.begin_save(), Err(TaskError::DateRange)));
    assert!(editor.in_flight.is_none() && editor.dirty());
    assert_eq!(editor.fields.start_date, "2026-11-04");
    editor.edit("due_date", "".to_owned()).unwrap();
    assert!(editor.begin_save().unwrap().is_some());
}

#[test]
fn test_refresh_preserves_dirty_fields_and_old_version_for_conflict_detection() {
    let mut editor = EditorSession::new(detail(1));
    let version = editor.detail.task.updated_at;
    editor.edit("title", "本地假草稿".to_owned()).unwrap();
    let mut changed = detail(1);
    changed.task.title = "外部假变更".to_owned();
    changed.task.updated_at += chrono::Duration::seconds(1);
    changed.is_overdue = true;
    editor.refresh(changed);
    assert_eq!(editor.fields.title, "本地假草稿");
    assert_eq!(editor.detail.task.updated_at, version);
    assert!(editor.detail.is_overdue);
}

#[test]
fn test_clean_refresh_updates_fields_and_read_only_session_rejects_edits() {
    let mut editor = EditorSession::new(detail(1));
    let mut changed = detail(1);
    changed.task.title = "新假标题".to_owned();
    changed.context.read_only = true;
    editor.refresh(changed);
    assert_eq!(editor.fields.title, "新假标题");
    assert_eq!(
        editor.edit("title", "不应保存".to_owned()),
        Err(TaskError::PlanArchived)
    );
    assert!(!editor.dirty());
}

#[test]
fn test_unknown_fields_and_same_values_do_not_make_editor_dirty() {
    let mut editor = EditorSession::new(detail(1));
    assert!(!editor.edit("unknown", "ignored".to_owned()).unwrap());
    assert!(!editor.edit("title", "假任务".to_owned()).unwrap());
    assert!(editor.begin_save().unwrap().is_none());
    assert_eq!(editor.generation, 0);
    assert!(!editor.edit("priority", "4".to_owned()).unwrap());
}

#[test]
fn test_committed_save_waits_for_database_reload_instead_of_resubmitting_old_version() {
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "已经提交的假标题".to_owned()).unwrap();
    let snapshot = editor.begin_save().unwrap().unwrap();
    editor.committed = true;
    editor.error = Some("模拟刷新失败".to_owned());
    assert!(editor.begin_save().unwrap().is_none());
    assert!(editor.unfinished());
    let mut ack = detail(1);
    ack.task = ack
        .task
        .patched(
            &snapshot.patch,
            ack.task.updated_at + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert!(editor.acknowledge(ack));
    assert!(!editor.unfinished());
}

#[test]
fn test_datetime_selection_is_one_version_and_clear_removes_the_whole_pair() {
    let mut editor = EditorSession::new(detail(1));
    let value = "2026-11-03T09:01:00".parse().unwrap();
    assert!(editor.select_datetime("start", Some(value)).unwrap());
    assert_eq!(editor.generation, 1);
    assert_eq!(editor.fields.start_date, "2026-11-03");
    assert_eq!(editor.fields.start_time, "09:01");
    let snapshot = editor.begin_save().unwrap().unwrap();
    assert!(matches!(snapshot.patch.start_date, Patch::Set(_)));
    assert!(matches!(snapshot.patch.start_time, Patch::Set(_)));
    editor.failed("模拟失败".to_owned());
    assert!(editor.select_datetime("start", None).unwrap());
    assert!(editor.fields.start_date.is_empty() && editor.fields.start_time.is_empty());
    assert!(!editor.dirty());
}

#[test]
fn test_same_day_inverted_picker_times_remain_in_draft_and_block_save() {
    let mut editor = EditorSession::new(detail(1));
    editor
        .select_datetime("start", Some("2026-11-03T16:00:00".parse().unwrap()))
        .unwrap();
    editor
        .select_datetime("due", Some("2026-11-03T15:30:00".parse().unwrap()))
        .unwrap();
    assert!(matches!(editor.begin_save(), Err(TaskError::DateRange)));
    assert!(editor.in_flight.is_none());
    assert_eq!(editor.fields.start_time, "16:00");
    assert_eq!(editor.fields.due_time, "15:30");
    editor
        .select_datetime("due", Some("2026-11-03T16:01:00".parse().unwrap()))
        .unwrap();
    assert!(editor.begin_save().unwrap().is_some());
}

#[test]
fn test_old_text_save_ack_preserves_a_newer_datetime_selection() {
    let mut editor = EditorSession::new(detail(1));
    editor.edit("title", "先提交的假标题".to_owned()).unwrap();
    let first = editor.begin_save().unwrap().unwrap();
    editor
        .select_datetime("due", Some("2026-11-03T15:31:00".parse().unwrap()))
        .unwrap();
    let mut ack = detail(1);
    ack.task = ack.task.patched(&first.patch, ack.task.updated_at).unwrap();
    editor.committed = true;
    editor.acknowledge(ack);
    assert_eq!(editor.fields.due_time, "15:31");
    assert!(editor.dirty());
    let next = editor.begin_save().unwrap().unwrap();
    assert!(matches!(next.patch.due_time, Patch::Set(_)));
}
