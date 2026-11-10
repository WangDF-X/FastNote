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

#[test]
fn test_preview_shortcut_is_scoped_and_reuses_the_protected_transition() {
    let window = include_str!("../ui/app-window.slint");
    let page = include_str!("../ui/notes/page.slint");
    assert!(window.contains("event.text == \"/\" && root.current_page == \"小记\""));
    assert!(window.contains("root.note-has-selection && !root.note-shortcut-blocked"));
    assert!(window.contains("root.note-toggle-preview();"));
    assert!(page.contains("root.delete-id != \"\" || root.confirm-discard"));
    assert!(page.contains("root.rename-id != \"\""));
}

#[test]
fn test_entering_preview_restores_keyboard_focus_without_crossing_note_dialogs() {
    let page = include_str!("../ui/notes/page.slint");
    let handler = page
        .split("changed preview-mode => {")
        .nth(1)
        .expect("preview mode must restore focus");
    assert!(handler.contains("root.preview-mode && root.has-selection && !root.shortcut-blocked"));
    assert!(handler.contains("focus-outside.focus();"));
}

#[test]
fn test_mode_hover_hint_floats_without_layout_space() {
    let page = include_str!("../ui/notes/page.slint");
    assert!(page.contains("if edit-mode.hovered || preview-mode-button.hovered: Rectangle"));
    assert!(page.contains("modes.absolute-position"));
    assert!(page.contains("text: \"Ctrl+/ 快速切换\""));
    assert!(!page.contains("width: 28px; font-size: 12px"));
}

#[test]
fn test_preview_headings_use_distinct_fonts_and_sizes() {
    let page = include_str!("../ui/notes/page.slint");
    assert!(page.contains("block.kind >= 1 && block.kind <= 6 ? \"SimHei\""));
    assert!(page.contains("\"Microsoft YaHei UI\""));
    assert!(page.contains("block.kind == 6 ? 15px : 14px"));
    assert!(page.contains("block.kind == 7 ? \"Consolas\""));
}
