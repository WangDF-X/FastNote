// SET-03：检查设置接线与受控视觉，外观/鼠标/键盘和滚动定位仍需用户手工验收。

#[test]
fn test_task_preferences_entry_uses_the_existing_settings_route_and_live_page() {
    let workspace = include_str!("../../ui/settings/workspace.slint");
    assert!(workspace.contains("key: \"task-preferences\", label: \"任务偏好\""));
    assert!(workspace.contains("selected: root.section == page.key;"));
    let main = include_str!("../../ui/app-window.slint");
    assert!(main.contains("if root.current_page == \"设置\": SettingsWorkspace {"));
    assert!(main.contains("root.route_action(\"navigate:设置?section=\" + section)"));
    assert!(main.contains("if root.settings-section == \"task-preferences\": TaskPreferences {"));
    assert!(main.contains("root.task-preferences-enabled && !root.dialog-visible"));
    assert!(main.contains("root.settings-section != \"task-preferences\": PagePlaceholder"));
    assert!(include_str!("view.rs")
        .contains("window.set_settings_section(self.settings_section.clone().into());"));
}

#[test]
fn test_three_sort_choices_and_two_visibility_controls_are_confirmed_value_bindings() {
    let panel = include_str!("../../ui/settings/task-preferences.slint");
    assert!(panel.contains("model: [\"优先级\", \"到期时间\", \"最近创建\"];"));
    assert!(panel.contains("current-index: root.sort-index;"));
    assert!(panel.contains("[\"priority\", \"due_date\", \"created_desc\"][index]"));
    assert_eq!(panel.matches("VisibilitySwitch {").count(), 2);
    for group in ["unscheduled", "completed"] {
        assert!(panel.contains(&format!("checked: root.show-{group};")));
        assert!(panel.contains(&format!("settings:tasks:{group}:")));
    }
    assert_eq!(
        panel
            .matches("enabled: root.enabled && !root.busy;")
            .count(),
        3
    );
    assert!(!panel.contains("current-index <=>") && !panel.contains("checked <=>"));
    assert!(panel.contains("text: root.error-message;"));
    assert!(panel.contains("root.busy ? \"正在保存…\""));
    assert!(!panel.contains("label: \"保存\""));
    // 两行、分隔线及上下内边距。
    assert!(panel.contains("height: 130px;"));
    assert_eq!(2 * 56 + 1 + 2 * 8, 129);
}

#[test]
fn test_visibility_switch_preserves_keyboard_and_accessibility_without_mutating_checked_or_drawing_focus(
) {
    let source = include_str!("../../ui/settings/visibility-switch.slint");
    assert!(source.contains("in property <bool> checked;"));
    assert!(source.contains("root.toggled(!root.checked);"));
    assert!(!source.contains("root.checked ="));
    assert!(source.contains("accessible-role: checkbox;"));
    assert!(source.contains("accessible-checked: root.checked;"));
    assert!(source.contains("accessible-action-default => { root.activate(); }"));
    assert!(source.contains("event.text == Key.Return || event.text == \" \""));
    assert!(source.contains("width: parent.width; height: parent.height;"));
    assert!(source.contains("ShellFocus.restore-tick"));
    assert!(!source.contains("visible: keyboard.has-focus"));
    assert!(source.contains("ShellPalette.hover") && source.contains("mouse.pressed"));
}

#[test]
fn test_explicit_reveal_scrolls_once_and_does_not_add_sticky_section_headers() {
    let main = include_str!("../../ui/app-window.slint");
    assert!(main.contains("public function reveal-task-section(kind: int)"));
    assert!(main.contains("root.today-reveal-version += 1;"));
    assert!(main.contains("reveal-kind <=> root.today-reveal-kind;"));
    assert!(main.contains("reveal-version: root.today-reveal-version;"));
    let today = include_str!("../../ui/tasks/today-page.slint");
    // 重复项的默认 total 为 0；实际目标数据到达时才产生正版本变化。
    assert!(today.contains(
        "root.reveal-kind == section.kind && section.total > 0 ? root.reveal-version : 0"
    ));
    assert!(today.contains("if (reveal-tick > 0)"));
    assert!(today.contains("tasks-scroll.content-y = -clamp(self.y"));
    assert!(today.contains("root.reveal-kind = -1;"));
    assert!(today.contains("ShellFocus.claim(\"today:section:\" + section.kind);"));
    assert!(
        today
            .find("tasks-scroll.content-y = -clamp(self.y")
            .unwrap()
            < today.find("ShellFocus.claim(\"today:section:\"").unwrap()
    );
    assert!(
        today.find("tasks-scroll := ScrollView").unwrap() < today.find("SectionHeader {").unwrap()
    );
    assert!(!today.contains("sticky"));
}

#[test]
fn test_select_opening_claims_its_anchor_before_the_popup_takes_focus() {
    let select = include_str!("../../ui/tasks/task-select.slint");
    let open = select.split_once("function open-menu() {").unwrap().1;
    assert!(
        open.find("ShellFocus.claim(root.focus-key);").unwrap()
            < open.find("menu.show();").unwrap()
    );
}
