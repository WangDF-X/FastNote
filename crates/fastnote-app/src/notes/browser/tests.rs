use super::*;

fn entry(id: &str, parent: Option<&str>, name: &str, is_folder: bool) -> NoteFileEntry {
    NoteFileEntry {
        id: id.into(),
        parent: parent.map(str::to_owned),
        name: name.into(),
        is_folder,
    }
}

fn tree() -> Vec<NoteFileEntry> {
    vec![
        entry("work", None, "工作", true),
        entry("ideas", Some("work"), "灵感", true),
        entry("note", Some("ideas"), "想法.md", false),
        entry("root-file", None, "收集", false),
        entry("other", None, "其他", true),
    ]
}

#[test]
fn test_history_restores_files_folders_and_root_without_duplicate_selection() {
    let tree = tree();
    let mut state = NoteBrowser::default();
    assert!(!state.can_back());
    assert!(!state.can_forward());
    assert!(!state.navigate(&tree, NoteNavigation::Parent));
    assert!(!state.navigate(&tree, NoteNavigation::Back));
    assert!(!state.navigate(&tree, NoteNavigation::Forward));
    assert!(!state.select(&tree, Some("missing")));
    assert!(!state.select(&tree, None));
    assert!(state.select(&tree, Some("work")));
    assert!(!state.select(&tree, Some("work")));
    assert!(state.select(&tree, Some("note")));
    assert!(state.navigate(&tree, NoteNavigation::Back));
    assert_eq!(state.selected(), Some("work"));
    assert!(state.navigate(&tree, NoteNavigation::Back));
    assert_eq!(state.selected(), None);
    assert!(!state.can_back());
    assert!(state.navigate(&tree, NoteNavigation::Forward));
    assert_eq!(state.selected(), Some("work"));
    assert!(state.navigate(&tree, NoteNavigation::Forward));
    assert_eq!(state.selected(), Some("note"));
    assert!(!state.can_forward());
}

#[test]
fn test_parent_navigation_and_new_selection_clear_forward_history() {
    let tree = tree();
    let mut state = NoteBrowser::default();
    state.select(&tree, Some("note"));
    assert!(state.navigate(&tree, NoteNavigation::Parent));
    assert_eq!(state.selected(), Some("ideas"));
    assert!(state.navigate(&tree, NoteNavigation::Parent));
    assert_eq!(state.selected(), Some("work"));
    assert!(state.navigate(&tree, NoteNavigation::Parent));
    assert_eq!(state.selected(), None);
    assert!(!state.navigate(&tree, NoteNavigation::Parent));
    state.navigate(&tree, NoteNavigation::Back);
    assert!(state.can_forward());
    state.select(&tree, Some("other"));
    assert!(!state.can_forward());
}

#[test]
fn test_creation_target_is_folder_self_or_file_parent_not_last_expanded_folder() {
    let tree = tree();
    let mut state = NoteBrowser::default();
    assert_eq!(state.creation_parent(&tree), None);
    for (id, expected) in [
        ("work", Some("work")),
        ("ideas", Some("ideas")),
        ("note", Some("ideas")),
        ("root-file", None),
    ] {
        state.select(&tree, Some(id));
        assert_eq!(state.creation_parent(&tree).as_deref(), expected);
    }
    assert_eq!(creation_parent(&tree, Some("missing")), None);
    assert_eq!(creation_parent(&tree, None), None);
}

#[test]
fn test_path_tracks_selected_item_and_ancestor_rename_or_move() {
    let mut tree = tree();
    assert_eq!(virtual_path(&tree, None), "小记");
    assert_eq!(virtual_path(&tree, Some("missing")), "小记");
    assert_eq!(
        virtual_path(&tree, Some("note")),
        "小记\\工作\\灵感\\想法.md"
    );
    tree[0].name = "项目".into();
    tree[1].parent = Some("other".into());
    assert_eq!(
        virtual_path(&tree, Some("note")),
        "小记\\其他\\灵感\\想法.md"
    );
    assert_eq!(virtual_path(&tree, Some("work")), "小记\\项目");
    assert!(in_subtree(&tree, "note", "other"));
    assert!(in_subtree(&tree, "note", "note"));
    assert!(!in_subtree(&tree, "note", "work"));
    assert!(!in_subtree(&tree, "missing", "work"));
}

#[test]
fn test_deleted_subtree_prunes_history_and_selects_surviving_parent() {
    let old = tree();
    let mut state = NoteBrowser::default();
    state.select(&old, Some("work"));
    state.select(&old, Some("ideas"));
    state.select(&old, Some("note"));
    state.select(&old, Some("root-file"));
    state.navigate(&old, NoteNavigation::Back);
    let tree: Vec<_> = old
        .iter()
        .filter(|entry| !in_subtree(&old, &entry.id, "ideas"))
        .cloned()
        .collect();
    state.reconcile(&old, &tree);
    assert_eq!(state.selected(), Some("work"));
    assert!(state.navigate(&tree, NoteNavigation::Forward));
    assert_eq!(state.selected(), Some("root-file"));
    state.reconcile(&tree, &[]);
    assert_eq!(state.selected(), None);
    assert!(!state.can_back());
    assert!(!state.can_forward());
}

#[test]
fn test_history_is_bounded_and_rename_refresh_preserves_node_identity() {
    let mut tree = tree();
    let mut state = NoteBrowser::default();
    for _ in 0..100 {
        state.select(&tree, Some("work"));
        state.select(&tree, Some("note"));
    }
    assert_eq!(state.back.len(), MAX_HISTORY);
    let old = tree.clone();
    tree[2].name = "新名称".into();
    state.reconcile(&old, &tree);
    assert_eq!(state.selected(), Some("note"));
    for _ in 0..MAX_HISTORY {
        assert!(state.navigate(&tree, NoteNavigation::Back));
    }
    assert!(!state.can_back());
    assert_eq!(state.forward.len(), MAX_HISTORY);
    for _ in 0..MAX_HISTORY {
        assert!(state.navigate(&tree, NoteNavigation::Forward));
    }
    assert_eq!(state.selected(), Some("note"));
    assert_eq!(state.back.len(), MAX_HISTORY);
}

#[test]
fn test_broken_parent_chains_do_not_loop_and_reconcile_falls_back_to_root() {
    let mut old = tree();
    old[0].parent = Some("ideas".into());
    let mut state = NoteBrowser::default();
    state.select(&old, Some("note"));
    state.reconcile(&old, &[]);
    assert_eq!(state.selected(), None);
    assert!(!in_subtree(&old, "note", "other"));
    assert!(!virtual_path(&old, Some("note")).is_empty());
}
