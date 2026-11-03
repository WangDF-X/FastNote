// 仅检查共享视觉令牌与源码接线，不创建窗口；外观、焦点与交互仍由用户验收。

fn pixel_token(name: &str) -> u32 {
    let prefix = format!("out property <length> {name}:");
    include_str!("../../ui/tasks/task-style.slint")
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .expect("shared task size token exists")
        .trim()
        .strip_suffix("px;")
        .expect("task size token uses logical pixels")
        .parse()
        .expect("task size token is a positive integer")
}

#[test]
fn test_task_text_uses_three_shared_sizes_and_one_font_including_popups() {
    assert_eq!(pixel_token("body"), 14);
    assert_eq!(pixel_token("caption"), 12);
    assert_eq!(pixel_token("heading"), 15);
    assert!(include_str!("../../ui/tasks/task-style.slint")
        .contains("font-family: \"Microsoft YaHei UI\";"));
    for (name, source) in [
        ("today", include_str!("../../ui/tasks/today-page.slint")),
        ("row", include_str!("../../ui/tasks/task-row.slint")),
        ("drawer", include_str!("../../ui/tasks/task-drawer.slint")),
        ("field", include_str!("../../ui/tasks/task-field.slint")),
        ("select", include_str!("../../ui/tasks/task-select.slint")),
        ("menu", include_str!("../../ui/tasks/select-popup.slint")),
        ("date", include_str!("../../ui/tasks/datetime-popup.slint")),
        ("day", include_str!("../../ui/tasks/calendar-day.slint")),
        (
            "section",
            include_str!("../../ui/tasks/section-header.slint"),
        ),
        ("counter", include_str!("../../ui/tasks/task-counter.slint")),
        (
            "icon-button",
            include_str!("../../ui/tasks/icon-button.slint"),
        ),
    ] {
        assert!(
            source.contains("font-family: TaskStyle.font-family;"),
            "{name}"
        );
        for line in source.lines().filter(|line| line.contains("font-size:")) {
            assert!(line.contains("font-size: TaskStyle."), "{name}: {line}");
        }
    }
    let button = include_str!("../../ui/components/action-button.slint");
    assert!(button.contains("in property <length> text-size: 14px;"));
}

#[test]
fn test_add_date_priority_and_destructive_actions_have_consistent_icons() {
    assert_eq!(pixel_token("icon-size"), 16);
    let today = include_str!("../../ui/tasks/today-page.slint");
    assert!(today.contains("icons/plus.svg\"); with-icon: true;"));
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains("icons/calendar.svg\"); with-icon: true;"));
    assert!(row.contains("glyph-with-icon: true; icon:"));
    assert!(row.contains("icons/flag.svg") && row.contains("icons/trash.svg"));
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    assert!(field.contains("icons/calendar.svg\"); with-icon: true;"));
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(picker.contains("icons/clock.svg") && picker.contains("icons/check.svg"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert!(drawer.contains("icons/trash.svg\"); with-icon: true;"));
    assert!(drawer.contains("icons/refresh.svg\"); with-icon: true;"));
    for icon in [
        include_str!("../../../../ui/icons/plus.svg"),
        include_str!("../../../../ui/icons/clock.svg"),
        include_str!("../../../../ui/icons/flag.svg"),
        include_str!("../../../../ui/icons/refresh.svg"),
    ] {
        assert!(icon.contains("viewBox=\"0 0 24 24\""));
        assert!(icon.contains("stroke-width=\"1.8\""));
    }
    let button = include_str!("../../ui/components/action-button.slint");
    assert!(button.contains("in property <bool> with-icon: false;"));
    assert!(button.contains("root.with-icon ? 22px : 0px"));
    assert!(button.contains("if root.with-icon: Image {"));
    assert!(button.contains("overflow: elide;"));
}

#[test]
fn test_compact_calendar_has_room_for_six_weeks_time_and_confirmation() {
    assert_eq!(pixel_token("popup-width"), 320);
    assert_eq!(pixel_token("popup-height"), 432);
    assert_eq!(pixel_token("day-height"), 34);
    let source = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(source.contains("width: TaskStyle.popup-width;"));
    assert!(source.contains("height: TaskStyle.popup-height;"));
    assert!(source.contains("padding: 16px; spacing: 8px;"));
    assert!(source.contains("spacing: 2px;"));
    assert!(source.contains("height: 18px; spacing: 2px;"));
    assert!(source.contains("height: 6 * TaskStyle.day-height + 10px;"));
    let control = pixel_token("control-height");
    let calendar = 18 + 2 + 6 * pixel_token("day-height") + 10;
    let rows = [control, calendar, 1, control, 20, 1, control];
    let required_height = 32 + rows.iter().sum::<u32>() + 8 * (rows.len() as u32 - 1);
    assert!(required_height <= pixel_token("popup-height"));
    let field = include_str!("../../ui/tasks/task-field.slint");
    // 继续用成熟的文本引擎，不能为换皮丢掉中文输入、撤销和剪贴板能力。
    assert!(field.contains("single := LineEdit {") && field.contains("multi := TextEdit {"));
}

#[test]
fn test_select_menus_virtualize_long_models_and_reset_scroll_before_reopening() {
    let style = include_str!("../../ui/tasks/task-style.slint");
    assert!(style.contains("out property <int> menu-visible-rows: 7;"));
    assert_eq!(pixel_token("control-height"), 32);
    assert_eq!(pixel_token("menu-min-width"), 128);
    let menu = include_str!("../../ui/tasks/select-popup.slint");
    assert!(menu.contains("list := ListView {"));
    assert!(menu.contains("for label[index] in root.model: Rectangle {"));
    assert!(menu.contains("min(TaskStyle.menu-visible-rows, root.model.length)"));
    assert!(menu.contains("root.model.length - TaskStyle.menu-visible-rows"));
    assert!(menu.contains("changed open-version => {"));
    assert!(menu.contains("list.content-y = -clamp(root.active-index - 3"));
    assert!(!menu.contains("VerticalLayout {"));
    let select = include_str!("../../ui/tasks/task-select.slint");
    assert!(select.contains("width: max(root.width, TaskStyle.menu-min-width);"));
    let open = select.split_once("function open-menu() {").unwrap().1;
    let current = open
        .find("root.active-index = clamp(root.current-index")
        .unwrap();
    let reset = open.find("root.open-version += 1;").unwrap();
    let show = open.find("menu.show();").unwrap();
    assert!(current < reset && reset < show);
    assert!(select.contains("open-version: root.open-version;"));
    assert!(select.contains("in property <int> current-index;"));
    assert!(!select.contains("current-index <=>"));
    assert!(menu.contains("Key.Escape || event.text == Key.Tab) { root.close(); return accept; }"));
}

#[test]
fn test_new_controls_share_palette_and_keep_focus_hover_pressed_and_disabled_states() {
    for source in [
        include_str!("../../ui/tasks/task-select.slint"),
        include_str!("../../ui/tasks/calendar-day.slint"),
        include_str!("../../ui/tasks/icon-button.slint"),
        include_str!("../../ui/components/action-button.slint"),
    ] {
        assert!(source.contains("ShellPalette.hover") && source.contains(".pressed"));
        assert!(source.contains("0.45;"));
        assert!(source.contains("keyboard.has-focus"));
        assert!(source.contains("width: parent.width; height: parent.height;"));
        assert!(!source.contains('#'));
    }
    let day = include_str!("../../ui/tasks/calendar-day.slint");
    assert!(day.contains("root.selected ? ShellPalette.on-accent"));
    assert!(day.contains("focus-on-tab-navigation: root.selected;"));
}
