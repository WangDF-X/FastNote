use super::*;

#[test]
fn test_rename_then_refresh_version_saves_draft_and_rejects_old_baseline() {
    let mut fixture = Fixture::new();
    let original = fixture.add(1, "已保存的假正文");
    let mut undo = NoteUndoState::default();
    fixture
        .execute(
            &mut undo,
            NoteCommand::Relocate {
                id: original.id.to_string(),
                parent: None,
                name: "新名称".into(),
            },
            now(),
        )
        .unwrap();
    let refreshed = fixture
        .store
        .note_detail(original.id)
        .unwrap()
        .unwrap()
        .note;
    assert_eq!(refreshed.content, original.content);
    assert!(refreshed.updated_at > original.updated_at);
    let command = |expected_updated_at| NoteCommand::Update {
        id: original.id,
        content: "后续假草稿".into(),
        expected_updated_at,
    };
    assert_eq!(
        fixture
            .execute(&mut undo, command(original.updated_at), now())
            .unwrap_err()
            .domain_error(),
        Some(NoteError::Conflict)
    );
    let saved = fixture
        .execute(&mut undo, command(refreshed.updated_at), now())
        .unwrap()
        .detail
        .unwrap()
        .note;
    assert_eq!(saved.content, "后续假草稿");
    let tree = fixture.store.note_file_tree().unwrap();
    assert_eq!(tree[0].name, "新名称");
}

#[test]
fn test_ancestor_rename_keeps_child_file_version_and_body() {
    let mut fixture = Fixture::new();
    let folder = id(2).to_string();
    let note = Note::new(id(3), "假正文".into(), now()).unwrap();
    fixture
        .store
        .create_note_folder(&folder, None, "原目录", now())
        .unwrap();
    fixture
        .store
        .create_note_file(&note, Some(&folder), "小记")
        .unwrap();
    fixture
        .store
        .relocate_note_entry(&folder, None, "新目录", now())
        .unwrap();
    assert_eq!(
        fixture.store.note_detail(note.id).unwrap().unwrap().note,
        note
    );
    let tree = fixture.store.note_file_tree().unwrap();
    assert_eq!(
        tree.iter().find(|entry| entry.id == folder).unwrap().name,
        "新目录"
    );
    assert_eq!(
        tree.iter()
            .find(|entry| entry.id == note.id.to_string())
            .unwrap()
            .parent
            .as_deref(),
        Some(folder.as_str())
    );
}
