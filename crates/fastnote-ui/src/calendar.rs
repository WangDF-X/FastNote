//! 日历格子的纯显示投影，不承担任务分类和时间计算。

use fastnote_core::derive::calendar::DayAgenda;

/// 日期格最多显示的未完成任务数量。
const MAX_PREVIEWS: usize = 3;

/// 日期格中的任务预览和弱完成计数。
#[derive(Debug, Default, Eq, PartialEq)]
pub struct CellPreview {
    /// 最多三条未完成任务标题。
    pub titles: Vec<String>,
    /// 未显示的未完成任务数量。
    pub remaining: usize,
    /// 已完成数量，超过九项显示为 9+。
    pub completed_label: String,
}

/// 按领域层已确定的分组顺序投影，完成项不显示标题。
pub fn cell_preview(agenda: &DayAgenda) -> CellPreview {
    let open = agenda
        .due
        .iter()
        .chain(&agenda.start)
        .chain(&agenda.ongoing);
    let count = agenda.due.len() + agenda.start.len() + agenda.ongoing.len();
    let completed_label = match agenda.completed.len() {
        0 => String::new(),
        1..=9 => format!("✓{}", agenda.completed.len()),
        _ => "✓9+".to_owned(),
    };
    CellPreview {
        titles: open
            .take(MAX_PREVIEWS)
            .map(|item| item.title.clone())
            .collect(),
        remaining: count.saturating_sub(MAX_PREVIEWS),
        completed_label,
    }
}

#[cfg(test)]
mod tests {
    use fastnote_core::{DateTime, Task, TaskContext, TaskId, Utc};

    use super::*;

    fn item(index: usize) -> fastnote_core::TaskSummary {
        let id: TaskId = format!("019a0fff-aaaa-7000-8000-{index:012x}")
            .parse()
            .unwrap();
        let now: DateTime<Utc> = "2026-11-07T12:00:00Z".parse().unwrap();
        let task = Task::new(id, format!("任务 {index}"), now).unwrap();
        fastnote_core::TaskSummary::from_task(&task, TaskContext::default())
    }

    #[test]
    fn test_preview_limits_open_tasks_and_caps_completed_count() {
        let agenda = DayAgenda {
            due: vec![item(1), item(2)],
            start: vec![item(3)],
            ongoing: vec![item(4), item(5)],
            completed: (6..16).map(item).collect(),
            overload: false,
        };
        let preview = cell_preview(&agenda);
        assert_eq!(preview.titles, ["任务 1", "任务 2", "任务 3"]);
        assert_eq!(preview.remaining, 2);
        assert_eq!(preview.completed_label, "✓9+");
    }

    #[test]
    fn test_empty_and_single_completion_labels() {
        assert_eq!(cell_preview(&DayAgenda::default()), CellPreview::default());
        let agenda = DayAgenda {
            completed: vec![item(1)],
            ..DayAgenda::default()
        };
        assert_eq!(cell_preview(&agenda).completed_label, "✓1");
    }
}
