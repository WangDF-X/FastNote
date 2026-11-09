#[test]
fn test_note_manual_save_is_scoped_and_uses_existing_flush_callback() {
    let page = include_str!("../ui/notes/page.slint");
    let window = include_str!("../ui/app-window.slint");
    assert!(page.contains("event.modifiers.control"));
    assert!(page.contains("root.flush(); return accept;"));
    assert!(page.contains("changed has-focus => { if (!self.has-focus) { root.flush(); } }"));
    assert!(window.contains("root.current_page == \"小记\" && root.note-has-selection"));
    assert!(window.contains("!root.note-create-open && root.note-rename-id == \"\""));
}

#[test]
fn test_directory_blank_and_expand_release_editor_focus_without_eating_operation() {
    let tree = include_str!("../ui/notes/tree.slint");
    let row = include_str!("../ui/notes/tree-row.slint");
    let page = include_str!("../ui/notes/page.slint");
    assert!(tree.contains("root.dismiss-editor-focus(); root.selected-entry(\"\");"));
    assert!(row.contains("keys.focus(); root.toggled();"));
    assert!(page.contains("dismiss-editor-focus => { focus-outside.focus(); }"));
}
