// 接线和布局源码契约测试：不创建窗口、模拟点击或截图；实际显示仍待用户手工验收。

#[test]
fn test_task_row_stretches_title_instead_of_centering_a_zero_width_body() {
    let source = include_str!("../../ui/tasks/task-row.slint");
    assert!(source.contains("alignment: stretch;"));
    assert!(!source
        .lines()
        .any(|line| line.trim() == "alignment: center;"));
    let body = source.split_once("body := Rectangle {").unwrap().1;
    assert!(body.contains("horizontal-stretch: 1;"));
    assert!(body.contains("min-width: 180px;"));
    assert!(body.contains("preferred-width: 320px;"));
    assert!(body.contains("text: root.data.title;"));
    let focus = body.split_once("body-keyboard := FocusScope {").unwrap().1;
    assert!(focus.contains("width: parent.width; height: parent.height;"));
}

#[test]
fn test_date_button_is_visible_and_connected_to_the_shared_task_drawer() {
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains("label: \"设置日期\";"));
    assert!(row.contains("activated => { root.dates-requested(); }"));
    let page = include_str!("../../ui/tasks/today-page.slint");
    assert!(page.contains("dates-requested => { root.dates-requested(task.id); }"));
    let window = include_str!("../../ui/app-window.slint");
    assert!(window.contains("dates-requested(id) => { root.task-open-dates(id); }"));
    assert!(window.contains("root.task-read-only ? \"editor:close\" : root.task-detail-focus"));
    assert!(window.contains("start-date <=> root.task-start-date;"));
    assert!(window.contains("due-date <=> root.task-due-date;"));
}

#[test]
fn test_drawer_and_text_fields_have_explicit_nonzero_geometry() {
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    let focus = drawer.split_once("FocusScope {").unwrap().1;
    assert!(focus.contains("width: parent.width; height: parent.height;"));
    assert!(!drawer
        .lines()
        .any(|line| line.trim() == "alignment: center;"));
    let field = include_str!("../../ui/tasks/task-field.slint");
    assert!(field.contains("min-height: root.multiline ? 200px : 36px;"));
    assert!(field.contains("horizontal-stretch: 1;"));
    assert!(field.contains("preferred-width: 200px;"));
}

#[test]
fn test_row_action_order_is_status_dates_priority_delete_without_duplicate_drawer_controls() {
    let row = include_str!("../../ui/tasks/task-row.slint");
    let status = row.find("status-choice := TaskSelect {").unwrap();
    let date = row.find("label: \"设置日期\";").unwrap();
    let priority = row.find("glyph: root.data.priority-label;").unwrap();
    let delete = row.find("label: \"删除：\"").unwrap();
    assert!(status < date && date < priority && priority < delete);
    assert!(row.contains("model: [\"待处理\", \"进行中\", \"等待中\", \"已完成\"];"));
    assert!(row.contains("current-index: root.data.status-index;"));
    assert!(row.contains("selected(index) => { root.status-changed(index); }"));
    assert!(!row.contains("current-index <=>"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert!(!drawer.contains("status-index") && !drawer.contains("priority-index"));
    assert!(!drawer.contains("ComboBox"));
}

#[test]
fn test_datetime_fields_use_selection_components_and_only_commit_the_combined_choice() {
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(field.contains("picker := TaskDateTimePopup {"));
    assert!(picker.contains("for day[index] in root.days: PickerDay {"));
    assert!(picker.contains("model: root.hours;") && picker.contains("model: root.minutes;"));
    for source in [field, picker] {
        assert!(!source.contains("LineEdit") && !source.contains("TextInput"));
        assert!(!source.contains("DatePickerPopup") && !source.contains("ComboBox"));
    }
    assert_eq!(picker.matches("root.confirmed(").count(), 1);
    assert!(picker.contains("activated => { root.apply-selection(); }"));
    let apply = picker.split_once("function apply-selection() {").unwrap().1;
    let apply = apply.split_once("function focus-day() {").unwrap().0;
    for part in ["date", "hour", "minute"] {
        assert!(apply.contains(&format!("let {part} = root.chosen-{part};")));
    }
    let confirmed = apply.find("root.confirmed(date, hour, minute);").unwrap();
    let close = apply.find("root.close();").unwrap();
    assert!(confirmed < close);
    let cancel = picker.split_once("label: \"取消\";").unwrap().1;
    let cancel = cancel.split_once("ActionButton {").unwrap().0;
    assert!(cancel.contains("activated => { root.close(); }"));
    assert!(!cancel.contains("root.confirmed("));
    assert!(
        field.contains("confirmed(date, hour, minute) => { root.selected(date, hour, minute); }")
    );
    assert!(field.contains("out property <bool> picker-open: picker.is-open;"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert!(drawer.contains("root.datetime-selected(\"start\", date, hour, minute)"));
    assert!(drawer.contains("root.datetime-selected(\"due\", date, hour, minute)"));
    assert!(!drawer.contains("placeholder: \"YYYY-MM-DD\""));
}

#[test]
fn test_mouse_keyboard_and_accessibility_selection_emit_before_popup_release() {
    let menu = include_str!("../../ui/tasks/select-popup.slint");
    let accept = menu
        .split_once("function accept-selection(index: int) {")
        .expect("all selection paths use the popup-owned acceptance function")
        .1;
    let accept = accept.split_once("function ensure-visible() {").unwrap().0;
    assert!(accept.contains("index >= 0 && index < root.model.length"));
    let chosen = accept.find("root.chosen(index);").unwrap();
    let close = accept.find("root.close();").unwrap();
    assert!(
        chosen < close,
        "closing first destroys the virtual row's weak parent"
    );
    assert_eq!(menu.matches("root.chosen(").count(), 1);
    assert!(menu.contains("clicked => { root.accept-selection(index); }"));
    assert!(menu.contains("accessible-action-default => { root.accept-selection(index); }"));
    assert!(menu.contains("root.accept-selection(root.active-index); return accept;"));
    assert!(menu.contains("Key.Escape || event.text == Key.Tab) { root.close(); return accept; }"));
    let select = include_str!("../../ui/tasks/task-select.slint");
    assert!(select.contains("chosen(index) => { root.selected(index); }"));
    let row = include_str!("../../ui/tasks/task-row.slint");
    assert!(row.contains("selected(index) => { root.status-changed(index); }"));
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    for part in ["hour", "minute"] {
        assert!(picker.contains(&format!(
            "selected(index) => {{ root.chosen-{part} = index; }}"
        )));
    }
}

#[test]
fn test_picker_reopening_snapshots_current_values_and_resets_every_temporary_choice() {
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    let open = field.split_once("activated => {").unwrap().1;
    let open = open.split_once("picker.show();").unwrap().0;
    for part in ["date", "hour", "minute"] {
        assert!(open.contains(&format!("root.snapshot-{part} = root.initial-{part};")));
    }
    assert!(open.contains("root.open-version += 1;"));
    assert!(field.contains("open-version: root.open-version;"));
    let popup = include_str!("../../ui/tasks/datetime-popup.slint");
    // Slint 1.18.1 对弹层根 changed is-open 有编译器缺陷，使用显式打开版本复位。
    assert!(!popup.contains("changed is-open"));
    let reset = popup.split_once("changed open-version => {").unwrap().1;
    let reset = reset.split_once("pure function two-digits").unwrap().0;
    for (target, initial) in [
        ("chosen-date", "initial-date"),
        ("view-month", "initial-date.month"),
        ("chosen-hour", "initial-hour"),
        ("chosen-minute", "initial-minute"),
    ] {
        assert!(reset.contains(&format!("root.{target} = root.{initial};")));
    }
    assert!(reset.contains("root.view-year = clamp(root.initial-date.year, TaskStyle.picker-first-year, TaskStyle.picker-last-year);"));
    assert!(reset.contains("year-choice.focus();"));
    assert!(!reset.contains("root.confirmed("));
}

#[test]
fn test_year_and_month_controls_update_the_combined_draft_instead_of_only_browsing() {
    let picker = include_str!("../../ui/tasks/datetime-popup.slint");
    assert!(picker.contains("selected(index) => { root.select-period(TaskStyle.picker-first-year + index, root.view-month); }"));
    assert!(
        picker.contains("selected(index) => { root.select-period(root.view-year, index + 1); }")
    );
    let select = picker
        .split_once("function select-period(year: int, month: int) {")
        .unwrap()
        .1;
    let select = select.split_once("Rectangle {").unwrap().0;
    assert!(select.contains("root.select-month(root.chosen-date, year, month)"));
    for field in [
        "chosen-date = date;",
        "view-year = date.year;",
        "view-month = date.month;",
    ] {
        assert!(select.contains(field));
    }
    assert!(!select.contains("root.confirmed("));
    assert!(!select.contains("chosen-hour =") && !select.contains("chosen-minute ="));
    let window = include_str!("../../ui/app-window.slint");
    assert!(window.contains("pure callback task-calendar-select-month(Date, int, int) -> Date;"));
    assert!(window.contains("return root.task-calendar-select-month(date, year, month);"));
    let drawer = include_str!("../../ui/tasks/task-drawer.slint");
    assert_eq!(
        drawer
            .matches("return root.calendar-select-month(date, year, month);")
            .count(),
        2
    );
    let field = include_str!("../../ui/tasks/datetime-field.slint");
    assert!(field.contains("return root.select-month(date, year, month);"));
    assert!(include_str!("presentation.rs")
        .contains("window.on_task_calendar_select_month(calendar::select_month);"));
}

#[test]
fn test_outside_click_dismissal_is_below_drawer_and_blocked_for_internal_popups_and_dialogs() {
    let window = include_str!("../../ui/app-window.slint");
    let outside = window.find("detail-outside := TouchArea {").unwrap();
    let drawer = window.find("task-drawer := TaskDrawer {").unwrap();
    let dialog = window.find("DialogHost {").unwrap();
    assert!(outside < drawer && drawer < dialog);
    let handler = &window[outside..drawer];
    assert!(handler.contains("enabled: !root.dialog-visible && !root.task-picker-open;"));
    assert!(handler.contains("clicked => { root.task-close(); }"));
    assert!(window.contains("picker-changed(open) => { root.task-picker-open = open; }"));
}
