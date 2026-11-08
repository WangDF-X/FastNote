// D-TODAY-07：只检查视觉策略与焦点接线，不创建窗口；外观与交互由用户手工验收。

fn focus_controls() -> [(&'static str, &'static str, &'static str); 10] {
    [
        (
            "components/action-button.slint",
            include_str!("../../ui/components/action-button.slint"),
            "keyboard",
        ),
        (
            "components/shell-control.slint",
            include_str!("../../ui/components/shell-control.slint"),
            "keyboard",
        ),
        (
            "components/theme-switch.slint",
            include_str!("../../ui/components/theme-switch.slint"),
            "keyboard",
        ),
        (
            "components/title-bar.slint",
            include_str!("../../ui/components/title-bar.slint"),
            "keyboard",
        ),
        (
            "components/page-banner.slint",
            include_str!("../../ui/components/page-banner.slint"),
            "keyboard",
        ),
        (
            "tasks/task-row.slint",
            include_str!("../../ui/tasks/task-row.slint"),
            "body-keyboard",
        ),
        (
            "tasks/task-select.slint",
            include_str!("../../ui/tasks/task-select.slint"),
            "keyboard",
        ),
        (
            "tasks/icon-button.slint",
            include_str!("../../ui/tasks/icon-button.slint"),
            "keyboard",
        ),
        (
            "tasks/section-header.slint",
            include_str!("../../ui/tasks/section-header.slint"),
            "keyboard",
        ),
        (
            "tasks/calendar-day.slint",
            include_str!("../../ui/tasks/calendar-day.slint"),
            "keyboard",
        ),
    ]
}

#[test]
fn test_focus_visual_policy_is_off_and_does_not_control_anchor_or_restore() {
    let source = include_str!("../../ui/shell-focus.slint");
    assert!(source.contains("out property <bool> show-indicators: false;"));
    // 固定视觉策略不能参与实际聚焦、恢复或输入能力的判断。
    assert_eq!(source.matches("show-indicators").count(), 1);
    assert!(source.contains("in-out property <string> anchor: \"\";"));
    assert!(source.contains("in-out property <int> restore-tick: 0;"));
    assert!(source.contains("if (key != \"\" && anchor != key)"));
    assert!(source.contains("anchor = key;"));
    assert!(source.contains("public function restore() {"));
    assert!(source.contains("restore-tick += 1;"));
}

#[test]
fn test_all_custom_focus_outlines_use_the_shared_disabled_visual_policy() {
    for (name, source, scope) in focus_controls() {
        let visibility = format!("visible: ShellFocus.show-indicators && {scope}.has-focus;");
        assert!(source.contains(&visibility), "{name}");
        let focus_lines: Vec<_> = source
            .lines()
            .filter(|line| line.contains("visible:") && line.contains(".has-focus"))
            .collect();
        let expected_count = if name == "tasks/task-row.slint" { 2 } else { 1 };
        assert_eq!(focus_lines.len(), expected_count, "{name}");
        assert!(
            focus_lines
                .iter()
                .all(|line| line.contains("ShellFocus.show-indicators &&")),
            "{name}"
        );
        if name == "tasks/task-row.slint" {
            assert!(
                source.contains("visible: ShellFocus.show-indicators && date-keyboard.has-focus;")
            );
        }
        assert!(
            source.contains(&format!("{scope} := FocusScope {{")),
            "{name}"
        );
        assert!(source.contains(&format!("{scope}.focus();")), "{name}");
        assert!(source.contains("key-pressed(event) =>"), "{name}");
        assert!(!source.contains("has-focus: false;"), "{name}");
    }
}

#[test]
fn test_selection_menu_and_theme_do_not_leave_ungated_blue_focus_decoration() {
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains("border-color: ShellFocus.show-indicators && root.selected ? ShellPalette.accent : transparent;"));
    assert!(row.contains("TaskStyle.row-background(root.data.status-index, root.data.overdue,"));
    let select = include_str!("../../ui/tasks/task-select.slint");
    assert!(select.contains("border-color: ShellFocus.show-indicators && menu.is-open ? ShellPalette.accent : ShellPalette.border;"));
    assert!(select.contains("out property <bool> has-focus: keyboard.has-focus || menu.is-open;"));
    assert!(select.contains("changed popup-open => { if (!popup-open) { keyboard.focus(); } }"));
    let theme = include_str!("../../ui/components/theme-switch.slint");
    assert!(theme.contains("colorize: mouse.has-hover || (ShellFocus.show-indicators && keyboard.has-focus) ? ShellPalette.accent : ShellPalette.text-secondary;"));
    let menu = include_str!("../../ui/floating-ball.slint");
    assert!(menu.contains("border-width: ShellFocus.show-indicators && root.active-index == index && keyboard.has-focus ? 1px : 0px;"));
    assert!(menu.contains("root.active-index == index || mouse.has-hover ? ShellPalette.hover"));
    assert!(menu.contains("root.active-index = 0; keyboard.focus();"));
    let day = include_str!("../../ui/tasks/calendar-day.slint");
    assert!(day.contains("background: root.selected ? ShellPalette.accent : mouse.pressed"));
    assert!(day.contains("border-width: root.data.today && !root.selected ? 1px : 0px;"));
    assert!(day.contains("focus-on-tab-navigation: root.selected;"));
}

#[test]
fn test_native_text_focus_underline_is_only_visually_covered_and_errors_stay_on_top() {
    let field = include_str!("../../ui/tasks/task-field.slint");
    let single = field.find("single := LineEdit {").unwrap();
    let multi = field.find("multi := TextEdit {").unwrap();
    let cover = field.find("native-underline-cover := Rectangle {").unwrap();
    let frame = field.find("neutral-frame := Rectangle {").unwrap();
    let error = field.find("if root.invalid: Rectangle {").unwrap();
    assert!(single < multi && multi < cover && cover < frame && frame < error);
    let cover_source = &field[cover..frame];
    for expected in [
        "x: 4px; y: parent.height - 2px;",
        "width: parent.width - 8px; height: 2px;",
        "background: ShellPalette.surface;",
        "visible: !ShellFocus.show-indicators;",
        "accessible-role: none;",
    ] {
        assert!(cover_source.contains(expected));
    }
    let frame_source = &field[frame..error];
    assert!(frame_source.contains("width: parent.width; height: parent.height;"));
    assert!(frame_source.contains("border-color: ShellPalette.border; background: transparent;"));
    assert!(frame_source.contains("border-width: 1px;"));
    assert!(frame_source.contains("visible: !ShellFocus.show-indicators;"));
    for overlay in [cover_source, frame_source] {
        for input_handler in [
            "TouchArea",
            "FocusScope",
            "key-pressed",
            ".focus()",
            "has-focus",
        ] {
            assert!(!overlay.contains(input_handler), "{input_handler}");
        }
    }
    assert!(field[error..].contains("border-color: ShellPalette.danger;"));
    assert!(!field[error..].contains("ShellFocus.show-indicators"));
    assert_eq!(field.matches("text <=> root.text;").count(), 2);
    assert!(field.contains("enabled: root.enabled && !root.multiline;"));
    assert!(field.contains("enabled: root.enabled && root.multiline;"));
    assert!(field.contains("root.multiline ? multi.has-focus : single.has-focus;"));
    let focus_handlers = field.split_once("changed focused => {").unwrap().1;
    let focus_handlers = focus_handlers.split_once("single := LineEdit {").unwrap().0;
    assert!(focus_handlers.contains("else { root.commit(); }"));
    assert!(
        focus_handlers.contains("multi.focus();") && focus_handlers.contains("single.focus();")
    );
    assert!(!focus_handlers.contains("ShellFocus.show-indicators"));
    assert!(field.contains("accepted(value) => { root.accepted(); }"));
}

#[test]
fn test_actual_focus_restoration_and_dialog_tab_cycle_do_not_depend_on_visual_policy() {
    for (name, source, _) in focus_controls() {
        assert!(source.contains("ShellFocus.restore-tick"), "{name}");
        assert!(source.contains("changed focus-tick =>"), "{name}");
        assert!(source.contains("ShellFocus.anchor"), "{name}");
    }
    let button = include_str!("../../ui/components/action-button.slint");
    assert!(button.contains("out property <bool> focused: keyboard.has-focus;"));
    assert!(button.contains("public function focus-button() {"));
    let dialog = include_str!("../../ui/components/dialog-host.slint");
    assert!(dialog.contains("if (confirm-button.focused)"));
    assert!(dialog.contains("if (cancel-button.focused)"));
    assert!(dialog.contains("confirm-button.focus-button();"));
    assert!(dialog.contains("cancel-button.focus-button();"));
    assert!(!dialog.contains("ShellFocus.show-indicators"));
}
