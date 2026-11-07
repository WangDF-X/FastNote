//! 仅校验设计接线和尺寸预算；外观、滚动及鼠标交互仍由用户手工验收。

const CELL: &str = include_str!("../../ui/calendar/cell.slint");
const STRIP: &str = include_str!("../../ui/calendar/task-strip.slint");
const PANEL: &str = include_str!("../../ui/calendar/day-panel.slint");
const PAGE: &str = include_str!("../../ui/calendar/page.slint");
const STYLE: &str = include_str!("../../ui/calendar/style.slint");
const BUTTON: &str = include_str!("../../ui/components/action-button.slint");

#[test]
fn test_calendar_navigation_keeps_intrinsic_button_width_and_matching_arrows() {
    for (name, label, rotation, action) in [
        ("previous-button", "\"上周\" : \"上月\"", "0deg", "previous"),
        ("next-button", "\"下周\" : \"下月\"", "180deg", "next"),
    ] {
        let button = PAGE
            .split_once(&format!("{name} := ActionButton {{"))
            .unwrap()
            .1
            .split_once("\n            }")
            .unwrap()
            .0;
        assert!(button.contains(label));
        assert!(button.contains("icon: @image-url(\"../../../../ui/icons/arrow-left.svg\");"));
        assert!(button.contains(&format!("icon-rotation: {rotation};")));
        assert!(button.contains(&format!("root.action(\"{action}\")")));
        assert!(!button.contains("width:"));
        assert!(!button.contains("text-size:"));
    }
    assert!(
        BUTTON.contains("width: max(72px, label-text.preferred-width + 32px + root.icon-space);")
    );
}

#[test]
fn test_arrow_rotation_is_centered_and_other_action_buttons_stay_unrotated() {
    assert!(BUTTON.contains("in property <angle> icon-rotation: 0deg;"));
    assert!(BUTTON.contains("transform-rotation: root.icon-rotation;"));
    assert!(BUTTON.contains("transform-origin: { x: self.width / 2, y: self.height / 2 };"));
    for deprecated in [
        "rotation-angle:",
        "rotation-origin-x:",
        "rotation-origin-y:",
    ] {
        assert!(!BUTTON.contains(deprecated));
    }
}

#[test]
fn test_calendar_title_has_no_description_and_preserves_state_legend() {
    assert!(PAGE.contains("text: root.heading;"));
    assert!(!PAGE.contains("任务状态一目了然"));
    for label in ["待处理", "进行中", "等待中", "已完成", "逾期"] {
        assert!(PAGE.contains(label));
    }
}

#[test]
fn test_cell_renders_completion_with_shared_vector_icon() {
    assert!(CELL.contains("if root.completed-label != \"\" && root.in-month: Rectangle"));
    assert!(CELL.contains("source: @image-url(\"../../../../ui/icons/check.svg\");"));
    assert!(CELL.contains("text: root.completed-label;"));
    assert!(CELL.contains("background: ShellPalette.task-done;"));
    assert!(!CELL.contains('✓'));
}

#[test]
fn test_grid_and_day_panel_use_shared_status_strips_not_group_colors() {
    assert!(CELL.contains("for task in root.items: CalendarTaskStrip"));
    assert!(CELL.contains("compact: true;"));
    assert!(PANEL.contains("for task in section.items: CalendarTaskStrip"));
    assert!(PANEL.contains("compact: false;"));
    assert!(STRIP.contains("TaskStyle.row-background(root.data.status-index, root.data.overdue, root.hovered, root.pressed)"));
    assert!(!STRIP.contains("section-background"));
    assert!(STRIP.contains("root.data.status-label"));
    assert!(STRIP.contains("root.data.overdue ? \"逾期 · \""));
    assert!(!STRIP.contains("TouchArea"));
}

#[test]
fn test_calendar_text_uses_readable_shared_fonts_and_neutral_colors() {
    assert!(STRIP.contains("font-size: TaskStyle.body;"));
    assert!(STRIP.contains("font-size: TaskStyle.caption;"));
    assert!(STRIP.contains("font-weight: 500;"));
    assert!(STRIP.contains("font-weight: root.data.done ? 400 : 600;"));
    assert!(STRIP.contains("color: ShellPalette.text-primary;"));
    assert!(STRIP.contains("color: ShellPalette.text-secondary;"));
    assert!(PANEL.contains("font-size: TaskStyle.heading;"));
    assert!(CELL.contains("font-size: TaskStyle.body;"));
    for source in [STRIP, CELL, PANEL, PAGE] {
        assert!(source.contains("font-family: TaskStyle.font-family;"));
        assert!(!source.contains("font-size: 11px"));
        assert!(
            !source.contains('#'),
            "calendar must reuse existing theme colors"
        );
    }
}

fn pixels(token: &str) -> usize {
    STYLE
        .split(&format!("{token}: "))
        .nth(1)
        .unwrap()
        .split("px")
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn test_month_cell_budget_fits_three_strips_and_overflow_without_small_text() {
    let spacing = pixels("cell-spacing");
    let content = 2 * pixels("cell-padding")
        + pixels("day-header-height")
        + super::MAX_PREVIEWS * pixels("preview-height")
        + (super::MAX_PREVIEWS - 1) * spacing
        + pixels("more-height")
        + 3 * spacing;
    assert!(pixels("cell-min-height") >= content);
    assert!(PAGE.contains("grid-scroll := ScrollView"));
    assert!(PAGE.contains("content-width: self.visible-width;"));
    assert!(PAGE.contains("CalendarStyle.cell-min-height + CalendarStyle.grid-gap"));
    assert!(PANEL.contains("scroll := ScrollView"));
    assert!(PANEL.contains("content-height: list.preferred-height;"));
    assert!(PANEL.contains("width: 320px;"));
}
