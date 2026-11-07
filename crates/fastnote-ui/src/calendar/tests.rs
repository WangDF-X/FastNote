use fastnote_core::{DateTime, Task, TaskContext, TaskId, TaskStatus, TaskSummary, Utc};
use slint::Model;

use super::*;

fn item(index: usize, status: TaskStatus) -> TaskSummary {
    let id: TaskId = format!("019a0fff-aaaa-7000-8000-{index:012x}")
        .parse()
        .unwrap();
    let now: DateTime<Utc> = "2026-11-07T12:00:00Z".parse().unwrap();
    let task = Task::new(id, format!("任务 {index}"), now)
        .unwrap()
        .with_status(status, now)
        .unwrap();
    TaskSummary::from_task(&task, TaskContext::default())
}

#[test]
fn test_preview_limits_open_tasks_and_caps_completed_count() {
    let agenda = DayAgenda {
        due: vec![item(1, TaskStatus::Todo), item(2, TaskStatus::Waiting)],
        start: vec![item(3, TaskStatus::InProgress)],
        ongoing: vec![
            item(4, TaskStatus::InProgress),
            item(5, TaskStatus::InProgress),
        ],
        completed: (6..16).map(|n| item(n, TaskStatus::Done)).collect(),
        overload: false,
    };
    let preview = cell_preview(&agenda, 2026);
    assert_eq!(
        preview
            .items
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["任务 1", "任务 2", "任务 3"]
    );
    assert_eq!(preview.remaining, 2);
    assert_eq!(preview.completed_label, "9+");
}

#[test]
fn test_empty_and_single_completion_labels() {
    assert_eq!(
        cell_preview(&DayAgenda::default(), 2026),
        CellPreview::default()
    );
    let agenda = DayAgenda {
        completed: vec![item(1, TaskStatus::Done)],
        ..DayAgenda::default()
    };
    assert_eq!(cell_preview(&agenda, 2026).completed_label, "1");
}

#[test]
fn test_completed_count_labels_do_not_include_font_dependent_icons() {
    for count in 1..=9 {
        let agenda = DayAgenda {
            completed: (0..count).map(|n| item(n, TaskStatus::Done)).collect(),
            ..DayAgenda::default()
        };
        let preview = cell_preview(&agenda, 2026);
        assert_eq!(preview.completed_label, count.to_string());
        assert!(preview.items.is_empty());
        assert_eq!(preview.remaining, 0);
    }
}

#[test]
fn test_preview_keeps_actual_state_overdue_and_date_metadata() {
    let mut task = item(1, TaskStatus::Waiting);
    task.is_overdue = true;
    task.waiting_days = Some(2);
    task.due_date = Some("2026-11-05".parse().unwrap());
    task.due_time = Some("09:30:00".parse().unwrap());
    let agenda = DayAgenda {
        due: vec![task],
        ..DayAgenda::default()
    };
    let preview = cell_preview(&agenda, 2026);
    let row = &preview.items[0];
    assert_eq!(row.status_index, 2);
    assert_eq!(row.status_label, "等待中");
    assert!(row.overdue);
    assert!(!row.done);
    assert_eq!(row.metadata, "11-05 09:30 · 已等待 2 天");
    assert_eq!(
        cell_preview(&agenda, 2027).items[0].metadata,
        "2026-11-05 09:30 · 已等待 2 天"
    );
    assert!(agenda.due[0].is_overdue);
    assert_eq!(agenda.due[0].status, Some(TaskStatus::Waiting));
}

#[test]
fn test_day_sections_keep_group_order_and_do_not_override_task_status() {
    let agenda = DayAgenda {
        due: vec![item(1, TaskStatus::Todo)],
        start: vec![item(2, TaskStatus::Waiting)],
        ongoing: vec![item(3, TaskStatus::InProgress)],
        completed: vec![item(4, TaskStatus::Done)],
        overload: false,
    };
    let sections = day_sections(&agenda, 2026);
    for (section, (kind, label, status)) in sections.iter().zip([
        (0, "到期", 0),
        (1, "开始", 2),
        (2, "进行中", 1),
        (3, "已完成", 3),
    ]) {
        assert_eq!(section.kind, kind);
        assert_eq!(section.label, label);
        assert_eq!(section.total, 1);
        assert_eq!(section.items.row_count(), 1);
        let row = section.items.row_data(0).unwrap();
        assert_eq!(row.status_index, status);
        assert_eq!(row.done, status == 3);
    }
    assert_eq!(sections.len(), 4);
}

#[test]
fn test_day_sections_hide_empty_groups_but_do_not_limit_detail_tasks() {
    assert!(day_sections(&DayAgenda::default(), 2026).is_empty());
    let agenda = DayAgenda {
        start: (0..5).map(|n| item(n, TaskStatus::Waiting)).collect(),
        ..DayAgenda::default()
    };
    let sections = day_sections(&agenda, 2026);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].kind, 1);
    assert_eq!(sections[0].total, 5);
    assert_eq!(sections[0].items.row_count(), 5);
    assert_eq!(sections[0].items.row_data(4).unwrap().title, "任务 4");
    assert_eq!(cell_preview(&agenda, 2026).remaining, 2);
}

#[test]
fn test_grid_and_day_panel_share_the_same_task_projection() {
    let mut task = item(1, TaskStatus::InProgress);
    task.is_overdue = true;
    task.start_date = Some("2026-11-01".parse().unwrap());
    task.due_date = Some("2026-11-05".parse().unwrap());
    let agenda = DayAgenda {
        due: vec![task],
        ..DayAgenda::default()
    };
    assert_eq!(
        cell_preview(&agenda, 2026).items[0],
        day_sections(&agenda, 2026)[0].items.row_data(0).unwrap()
    );
}
