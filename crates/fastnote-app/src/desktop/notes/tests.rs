use super::*;
use fastnote_core::{Note, NoteDetail};

const FILE_ID: &str = "019a0fff-aaaa-7000-8000-000000000001";

fn session() -> NoteSession {
    let id = FILE_ID.parse().unwrap();
    let editor = NoteEditor::new(NoteDetail {
        note: Note::new(
            id,
            "测试正文".into(),
            "2026-11-09T08:00:00Z".parse().unwrap(),
        )
        .unwrap(),
    });
    NoteSession {
        editor: Some(editor),
        tree: vec![
            NoteFileEntry {
                id: "folder".into(),
                parent: None,
                name: "文件夹".into(),
                is_folder: true,
            },
            NoteFileEntry {
                id: FILE_ID.into(),
                parent: Some("folder".into()),
                name: "测试小记".into(),
                is_folder: false,
            },
            NoteFileEntry {
                id: "other".into(),
                parent: None,
                name: "其他".into(),
                is_folder: true,
            },
        ],
        ..Default::default()
    }
}

#[test]
fn test_directory_navigation_keeps_active_file_dirty_content_and_pending_save_unchanged() {
    let mut session = session();
    let editor = session.editor.as_mut().unwrap();
    editor.edit("尚未保存的测试草稿".into());
    editor.pending = Some(7);
    editor.error = Some("测试失败".into());
    session.after_save = Some("open:other".into());
    session.select(Some(FILE_ID));
    session.select(Some("other"));
    for direction in [
        NoteNavigation::Back,
        NoteNavigation::Parent,
        NoteNavigation::Forward,
        NoteNavigation::Back,
    ] {
        session.navigate(direction);
        let editor = session.editor.as_ref().unwrap();
        assert_eq!(editor.detail.note.id.to_string(), FILE_ID);
        assert_eq!(editor.detail.note.content, "测试正文");
        assert_eq!(editor.content, "尚未保存的测试草稿");
        assert_eq!(editor.pending, Some(7));
        assert_eq!(editor.error.as_deref(), Some("测试失败"));
        assert_eq!(session.after_save.as_deref(), Some("open:other"));
        assert_eq!(session.detail_request, 0);
        assert_eq!(session.pending_open, None);
    }
}

#[test]
fn test_directory_selection_expansion_and_navigation_keep_preview_mode_and_saved_file() {
    let mut session = session();
    session.preview = true;
    session.select(Some(FILE_ID));
    session.tree_state.toggle(&session.tree, "folder");
    assert_eq!(session.browser.selected(), Some(FILE_ID));
    assert_eq!(session.tree_state.visible(&session.tree).len(), 2);
    session.select(Some("other"));
    session.navigate(NoteNavigation::Back);
    assert_eq!(session.browser.selected(), Some(FILE_ID));
    assert_eq!(session.tree_state.visible(&session.tree).len(), 3);
    assert!(session.preview);
    let editor = session.editor.as_ref().unwrap();
    assert_eq!(editor.detail.note.id.to_string(), FILE_ID);
    assert!(!editor.protected());
    session.navigate(NoteNavigation::Parent);
    assert_eq!(session.browser.selected(), Some("folder"));
    assert!(session.preview);
}

#[test]
fn test_create_target_snapshot_does_not_follow_later_navigation_or_expansion() {
    let mut session = session();
    session.select(Some(FILE_ID));
    session.create_parent = session.browser.creation_parent(&session.tree);
    session.select(Some("other"));
    session.tree_state.toggle(&session.tree, "other");
    session.navigate(NoteNavigation::Parent);
    assert_eq!(session.create_parent.as_deref(), Some("folder"));
}

#[test]
fn test_tree_refresh_prunes_deleted_positions_without_touching_unrelated_editor() {
    let mut session = session();
    session.select(Some("other"));
    let tree = session
        .tree
        .iter()
        .filter(|entry| entry.id != "other")
        .cloned()
        .collect();
    session.replace_tree(tree);
    assert_eq!(session.browser.selected(), None);
    assert_eq!(
        session.editor.as_ref().unwrap().detail.note.id.to_string(),
        FILE_ID
    );
    assert!(!session.browser.can_back());
}
