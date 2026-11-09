use super::*;
use fastnote_core::{Note, NoteId};

fn detail(content: &str) -> NoteDetail {
    let id: NoteId = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    NoteDetail {
        note: Note::new(
            id,
            content.to_owned(),
            "2026-11-09T08:00:00Z".parse().unwrap(),
        )
        .unwrap(),
    }
}

#[test]
fn test_late_save_ack_retains_newer_input_and_uses_new_baseline_for_next_save() {
    let mut editor = NoteEditor::new(detail("原假正文"));
    editor.edit("第一次输入".to_owned());
    editor.pending = Some(10);
    editor.edit("保存途中继续输入".to_owned());
    let mut saved = detail("第一次输入");
    saved.note.updated_at += chrono::Duration::seconds(1);
    assert!(!editor.acknowledge(9, saved.clone()));
    assert!(editor.acknowledge(10, saved.clone()));
    assert_eq!(editor.content, "保存途中继续输入");
    assert!(editor.dirty());
    assert_eq!(editor.baseline_time(), saved.note.updated_at);
    let SavePlan::Submit(NoteCommand::Update {
        expected_updated_at,
        content,
        ..
    }) = editor.save_plan()
    else {
        panic!("new save expected")
    };
    assert_eq!(expected_updated_at, saved.note.updated_at);
    assert_eq!(content, editor.content);
}

#[test]
fn test_empty_overlong_and_failed_save_protect_draft_without_implicit_delete() {
    let mut editor = NoteEditor::new(detail("原假正文"));
    assert!(matches!(editor.save_plan(), SavePlan::Clean));
    editor.edit(" \n".to_owned());
    assert!(matches!(editor.save_plan(), SavePlan::Submit(_)));
    {
        let (content, expected) = ("字".repeat(20_001), NoteError::TooLong);
        editor.edit(content.clone());
        assert!(editor.protected());
        assert!(matches!(editor.save_plan(), SavePlan::Invalid(error) if error == expected));
        editor.refresh(detail("后台读取"));
        assert_eq!(editor.content, content);
    }
    editor.edit("待保存".to_owned());
    editor.pending = Some(10);
    assert!(matches!(editor.save_plan(), SavePlan::Waiting));
    editor.fail(10, "保存失败".to_owned());
    assert!(editor.protected());
    assert_eq!(editor.detail.note.content, "原假正文");
    assert_eq!(editor.content, "待保存");
    assert!(matches!(editor.save_plan(), SavePlan::Submit(_)));
}

#[test]
fn test_discard_rejects_inflight_save_and_restores_confirmed_content_after_failure() {
    let mut editor = NoteEditor::new(detail("已保存"));
    editor.edit("草稿".into());
    editor.pending = Some(3);
    assert!(!editor.discard());
    assert_eq!(editor.content, "草稿");
    editor.fail(3, "失败".into());
    assert!(editor.discard());
    assert_eq!(editor.content, "已保存");
    assert!(!editor.protected());
}
