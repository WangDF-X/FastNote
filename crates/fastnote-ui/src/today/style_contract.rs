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
fn test_task_text_uses_shared_sizes_and_distinct_date_font() {
    assert_eq!(pixel_token("body"), 14);
    assert_eq!(pixel_token("caption"), 12);
    assert_eq!(pixel_token("heading"), 15);
    assert!(include_str!("../../ui/tasks/task-style.slint")
        .contains("font-family: \"Microsoft YaHei UI\";"));
    assert!(
        include_str!("../../ui/tasks/task-style.slint").contains("date-font-family: \"Segoe UI\";")
    );
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
    assert!(row.contains("icons/calendar.svg\");"));
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
fn test_select_menus_initialize_visible_selection_when_the_popup_is_created() {
    let style = include_str!("../../ui/tasks/task-style.slint");
    assert!(style.contains("out property <int> menu-visible-rows: 7;"));
    assert_eq!(pixel_token("control-height"), 32);
    assert_eq!(pixel_token("menu-min-width"), 128);
    let menu = include_str!("../../ui/tasks/select-popup.slint");
    assert!(menu.contains("list := ListView {"));
    assert!(menu.contains("for label[index] in root.model: Rectangle {"));
    assert!(menu.contains("min(TaskStyle.menu-visible-rows, root.model.length)"));
    assert!(menu.contains("root.model.length - TaskStyle.menu-visible-rows"));
    assert!(menu.contains("content-y: -clamp(root.selected-index - 3"));
    assert!(!menu.contains("changed open-version"));
    assert!(!menu.contains("VerticalLayout {"));
    let select = include_str!("../../ui/tasks/task-select.slint");
    assert!(select.contains("width: max(root.width, TaskStyle.menu-min-width);"));
    let open = select.split_once("function open-menu() {").unwrap().1;
    let current = open
        .find("root.active-index = clamp(root.current-index")
        .unwrap();
    let show = open.find("menu.show();").unwrap();
    assert!(current < show);
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

#[test]
fn test_overdue_background_takes_priority_without_changing_status_text_or_adding_a_blue_outline() {
    let style = include_str!("../../ui/tasks/task-style.slint");
    for expression in [
        "status == 1 ? ShellPalette.task-progress",
        "status == 2 ? ShellPalette.task-done",
        ": ShellPalette.task-todo;",
    ] {
        assert!(style.contains(expression));
    }
    let overdue = style.find("overdue ? ShellPalette.danger-subtle").unwrap();
    let status = style
        .find("status == 1 ? ShellPalette.task-progress")
        .unwrap();
    assert!(overdue < status);
    assert_eq!(pixel_token("row-radius"), 8);
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains(
        "background: TaskStyle.row-background(root.data.status-index, root.data.overdue,"
    ));
    assert!(row.contains("border-color: ShellFocus.show-indicators && root.selected ? ShellPalette.accent : transparent;"));
    assert!(!row.contains("ShellPalette.selected"));
    assert!(!row.contains("root.data.done ? ShellPalette.text-"));
    assert!(row.contains("color: ShellPalette.text-primary;"));
    let select = include_str!("../../ui/tasks/task-select.slint");
    assert!(select.contains("root.quiet ? transparent : ShellPalette.surface"));
}

#[test]
fn test_task_instructions_are_removed_without_disabling_shortcuts_or_safety_feedback() {
    let page = include_str!("../../ui/tasks/today-page.slint");
    for text in [
        "回车添加",
        "按 Enter 添加",
        "点击标题编辑详情",
        "让安排从这里开始",
    ] {
        assert!(!page.contains(text));
    }
    assert!(page.contains("placeholder: \"添加一个任务\";"));
    assert!(page.contains("accepted => { root.created(root.quick-title); }"));
    assert!(page.contains("text: root.create-error;"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert!(!drawer.contains("点击详情外部，保存后关闭"));
    assert!(!drawer.contains("日期与时间一起确认，不自动提醒"));
    for text in [
        "开始时间",
        "到期时间",
        "root.save-label",
        "root.error-message",
        "仅供查看",
        "重试保存",
    ] {
        assert!(drawer.contains(text));
    }
    assert!(drawer.contains("activated => { root.deleted(); }"));
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    assert!(field.contains("accessible-label: root.label + \"日期和时间，\""));
    assert!(
        field.contains("confirmed(date, hour, minute) => { root.selected(date, hour, minute); }")
    );
}

#[test]
fn test_all_task_sections_and_status_texts_stay_neutral_while_icons_keep_semantic_colors() {
    for source in [
        include_str!("../../ui/tasks/section-header.slint"),
        include_str!("../../ui/tasks/task-counter.slint"),
        include_str!("../../ui/tasks/task-row.slint"),
        include_str!("../../ui/tasks/icon-button.slint"),
    ] {
        for block in source.split("Text {").skip(1) {
            let block = block.split_once('}').unwrap().0;
            let color = block
                .split_once("color:")
                .unwrap()
                .1
                .split_once(';')
                .unwrap()
                .0
                .trim();
            assert!(
                matches!(
                    color,
                    "ShellPalette.text-primary" | "ShellPalette.text-secondary"
                ),
                "status text must be neutral: {color}"
            );
        }
    }
    let section = include_str!("../../ui/tasks/section-header.slint");
    assert!(section.contains("background: TaskStyle.section-background(root.data.kind,"));
    assert!(section.contains("colorize: TaskStyle.section-icon-color(root.data.kind);"));
    assert!(section.contains("icons/check-circle.svg") && section.contains("icons/refresh.svg"));
    assert!(!section.contains("icons/pause.svg"));
    let counter = include_str!("../../ui/tasks/task-counter.slint");
    assert!(counter.contains("root.tone == 1 ? ShellPalette.danger-subtle"));
    assert!(counter.contains("root.tone == 2 ? ShellPalette.task-done"));
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains("if root.data.overdue: Rectangle {"));
    assert!(row.contains("colorize: TaskStyle.row-danger;"));
    let button = include_str!("../../ui/components/action-button.slint");
    assert!(button.contains("in property <color> icon-color: root.text-color;"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert!(
        drawer.contains("text-color: ShellPalette.text-primary; icon-color: ShellPalette.danger;")
    );
}

#[test]
fn test_row_keeps_overdue_by_title_and_status_control_visibly_bordered() {
    let row = include_str!("../../ui/tasks/task-row.slint");
    let body = row.split_once("body := Rectangle {").unwrap().1;
    let body = body.split_once("status-choice := TaskSelect {").unwrap().0;
    assert!(body.contains("text: \"已逾期\""));
    assert!(
        body.contains("x: title.x + min(title.width, title.preferred-width) + 8px; y: title.y;")
    );
    let status = row.split_once("status-choice := TaskSelect {").unwrap().1;
    let status = status.split_once("date-entry := Rectangle {").unwrap().0;
    assert!(!status.contains("quiet: true"));
    assert!(status.contains("max-width: 90px;"));
    assert!(row.find("text: \"已逾期\"").unwrap() < row.find("date-entry := Rectangle {").unwrap());
    assert!(row.contains("TaskStyle.date-font-family"));
}

#[test]
fn test_year_sensitive_formatting_is_shared_without_replacing_raw_editor_dates() {
    let presenter = include_str!("presentation.rs");
    assert!(presenter.contains("let current_year = view.date.year();"));
    assert!(presenter.contains("row_data(task, current_year)"));
    assert!(presenter.contains("window.on_task_date_label("));
    let window = include_str!("../../ui/app-window.slint");
    assert!(window.contains("pure callback task-date-label(string, int) -> string;"));
    assert!(window.contains("return root.task-date-label(date, year);"));
    assert!(window.contains("start-date <=> root.task-start-date;"));
    assert!(window.contains("due-date <=> root.task-due-date;"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert_eq!(
        drawer
            .matches("date-label(date, year) => { return root.date-label(date, year); }")
            .count(),
        2
    );
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    assert!(field.contains("root.date-label(root.date-text, root.today.year)"));
    assert!(field.contains("accessible-label: root.label + \"日期和时间，\""));
    assert!(!field.contains("root.date-text = "));
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(picker.contains("root.date-label(root.chosen-date.year"));
    assert!(picker.contains("root.today.year) + \"  \""));
    assert!(picker.contains(
        "model: root.years; current-index: root.view-year - TaskStyle.picker-first-year;"
    ));
}

#[test]
fn test_year_range_and_initial_focus_are_consistent_with_the_existing_date() {
    use super::calendar::{PICKER_FIRST_YEAR, PICKER_LAST_YEAR};

    let style = include_str!("../../ui/tasks/task-style.slint");
    assert!(style.contains(&format!(
        "out property <int> picker-first-year: {PICKER_FIRST_YEAR};"
    )));
    assert!(style.contains(&format!(
        "out property <int> picker-last-year: {PICKER_LAST_YEAR};"
    )));
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(picker.contains("forward-focus: year-choice;"));
    assert!(picker.contains("year-choice := TaskSelect {"));
    assert!(picker.contains(
        "clamp(root.initial-date.year, TaskStyle.picker-first-year, TaskStyle.picker-last-year)"
    ));
    assert!(picker
        .contains("enabled: root.view-year > TaskStyle.picker-first-year || root.view-month > 1;"));
    assert!(picker
        .contains("enabled: root.view-year < TaskStyle.picker-last-year || root.view-month < 12;"));
    assert!(!picker.contains("1900") && !picker.contains("9999"));
}
