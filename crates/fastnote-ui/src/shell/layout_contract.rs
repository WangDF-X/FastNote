// D-SHELL-02：源码接线契约不创建窗口，不能替代用户的布局和键盘手工验收。
const MAIN: &str = include_str!("../../ui/app-window.slint");
const SIDEBAR: &str = include_str!("../../ui/components/sidebar-date.slint");
const FRAME: &str = include_str!("../../ui/components/page-frame.slint");

#[test]
fn test_six_modules_share_headerless_content_and_keep_the_window_caption() {
    assert!(!MAIN.contains("PageHeader"));
    assert!(!MAIN.contains("page-header"));
    assert_eq!(MAIN.matches("PageFrame {").count(), 1);
    assert!(MAIN.contains("caption-height: root.custom-titlebar ? 40px : 0px;"));
    assert!(MAIN.contains("if root.custom-titlebar: TitleBar {"));
    assert!(MAIN.contains("root.route_action(\"desktop:close\")"));
    assert!(MAIN.contains("root.route_action(\"desktop:maximize\")"));
    assert!(!MAIN.contains("root.caption-height + 64px"));
    let drawer = MAIN.split_once("task-drawer := TaskDrawer {").unwrap().1;
    assert!(drawer.contains("y: root.caption-height;"));
    assert!(drawer.contains("width: 400px;"));
}

#[test]
fn test_sidebar_date_replaces_brand_name_and_has_larger_left_aligned_logo() {
    assert!(MAIN.contains("width: 208px;"));
    assert!(MAIN.contains("date-label: root.sidebar-date;"));
    assert!(MAIN.contains("weekday-label: root.sidebar-weekday;"));
    assert!(!SIDEBAR.contains("FastNote"));
    for expected in [
        "height: 64px;",
        "x: 12px;",
        "y: 14px;",
        "width: 36px;",
        "height: 36px;",
        "source: ShellPalette.brand-icon;",
        "x: 58px;",
        "height: 22px;",
        "height: 18px;",
        "font-size: 16px;",
        "font-size: 12px;",
        "font-weight: 600;",
        "color: ShellPalette.text-primary;",
        "color: ShellPalette.text-secondary;",
        "width: parent.width - self.x - 12px;",
    ] {
        assert!(SIDEBAR.contains(expected), "sidebar-date.slint: {expected}");
    }
    for forbidden in [
        "calendar-selected",
        "today-date",
        "TouchArea",
        "FocusScope",
        "overflow: elide",
    ] {
        assert!(
            !SIDEBAR.contains(forbidden),
            "sidebar-date.slint: {forbidden}"
        );
    }
}

#[test]
fn test_return_action_is_inside_the_card_and_remains_available_for_all_page_states() {
    let back = FRAME
        .find("if root.can-go-back: HorizontalLayout {")
        .unwrap();
    let content = FRAME.find("visible: root.shows-content;").unwrap();
    assert!(back < content);
    let row = &FRAME[back..content];
    assert!(row.contains("height: 48px;"));
    assert!(row.contains("padding: 8px;"));
    assert!(row.contains("label: \"返回\";"));
    assert!(row.contains("ghost: true;"));
    assert!(row.contains("focus-key: root.back-focus-key;"));
    assert!(row.contains("activated => { root.back(); }"));
    assert!(!row.contains("border-width"));
    for state in ["loading", "empty", "error"] {
        assert!(FRAME[content..].contains(&format!("if root.state == PageState.{state}")));
    }
    assert!(MAIN.contains("back => { root.route_action(\"back\"); }"));
    assert!(MAIN.contains("back-focus-key: root.plan-detail ? \"plan-back\" : \"settings-back\";"));
}

#[test]
fn test_route_focus_moves_to_non_tab_content_without_stealing_dialog_focus() {
    let focus = MAIN
        .split_once("public function focus-route-content() {")
        .unwrap()
        .1;
    let focus = focus.split_once("init =>").unwrap().0;
    assert!(focus.contains("if (!root.dialog-visible)"));
    assert!(focus.contains("ShellFocus.anchor = \"\";"));
    assert!(focus.contains("workspace.focus();"));
    assert!(!MAIN.contains("focus-route-header"));
    let content = MAIN.split_once("workspace := FocusScope {").unwrap().1;
    assert!(content.contains("focus-on-tab-navigation: false;"));
    assert!(content.contains("accessible-label: root.page-title + \"内容区\";"));
    assert!(include_str!("mod.rs").contains("window.invoke_focus_route_content();"));
}

#[test]
fn test_header_removal_keeps_business_controls_and_feedback_bindings() {
    for expected in [
        "open-count: root.today-open-count;",
        "overdue-count: root.today-overdue-count;",
        "completed-count: root.today-completed-count;",
        "heading: root.calendar-heading;",
        "SettingsWorkspace {",
        "banner-visible: root.banner-visible;",
        "save-label: root.task-save-label;",
        "context-label: root.task-context;",
    ] {
        assert!(MAIN.contains(expected), "app-window.slint: {expected}");
    }
    let date = include_str!("date.rs");
    for forbidden in [
        "SystemClock",
        "Local::now",
        "request_today",
        "ensure_ui",
        "Timer::",
    ] {
        assert!(!date.contains(forbidden), "shell/date.rs: {forbidden}");
    }
    assert!(include_str!("view.rs").contains("super::date::apply_date(window, self.local_today)"));
    assert!(include_str!("handle.rs").contains("super::date::apply_date(&self.window, date)"));
}
#[test]
fn test_content_card_expands_with_window_and_keeps_side_margins() {
    let frame = include_str!("../../ui/components/page-frame.slint");
    assert!(frame.contains("x: (parent.width - self.width) / 2;"));
    assert!(frame.contains("width: parent.width - 48px;"));
    assert!(!frame.contains("min(1200px"));
}
